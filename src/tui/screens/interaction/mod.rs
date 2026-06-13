//! Interaction screen (#736) — chat-style transcript + multi-line input.
//!
//! UI scaffolding only. Binds to one `InteractionSession`'s history and
//! renders it. Per-turn agent spawning lands in #737; the rich keymap and
//! re-entry wiring land in #738. This screen sends no prompts.

mod diff_review;
mod diff_review_draw;
mod history;
mod input;
mod keymap;
#[cfg(test)]
mod keymap_tests;
mod layout;
pub(crate) mod lifecycle;
mod render;
mod scroll;
#[cfg(test)]
mod terminator_tests;
#[cfg(test)]
mod tests;
mod turn;
pub(crate) mod view_state;

use super::{Screen, ScreenAction};
use crate::session::interaction::{TurnRecord, TurnState};
use crate::tui::activity_log::LogLevel;
use crate::tui::navigation::InputMode;
use crate::tui::theme::Theme;
use chrono::Utc;
use crossterm::event::{Event, KeyEvent, KeyEventKind};
use keymap::{InteractionIntent, classify, pushup_prompt};
use layout::{HEADER_HEIGHT, INPUT_HEIGHT, effective_offset, inset_x};
use lifecycle::{Clock, RealClock};
use ratatui::{Frame, layout::Rect, style::Style};
use std::path::PathBuf;
use std::time::Instant;
use tui_textarea::TextArea;
use view_state::{CloseReason, InteractionView};

/// Tag used for every Interaction activity-log line.
const LOG_TAG: &str = "INTERACTION";

/// Map a lifecycle transition to the screen-action log line (#742): pinned
/// format + severity from `InteractionActivity`, mirrored into tracing.
pub(crate) fn activity_action(
    activity: &crate::work::activity::InteractionActivity,
) -> ScreenAction {
    activity.emit_tracing();
    ScreenAction::LogActivity {
        tag: activity.tag().to_string(),
        message: activity.message(),
        level: activity.severity().into(),
    }
}

/// Dedicated chat-style screen for a long-lived interaction session.
pub struct InteractionScreen {
    /// Issue this session is attached to. Keys re-entry + activity-log lines.
    issue_number: u64,
    /// Launch-time "Produce PR" choice. Gates the `Ctrl+P` pushup chord.
    produce_pr: bool,
    /// Worktree shown in the quit-confirm modal ("kept for manual inspection").
    worktree_path: PathBuf,
    /// Per-frame projection of the live `Session` (#950): transcript +
    /// `turn_state` (input lock) + `settled_from`/`pr_linked` (status banner).
    /// Refreshed via [`Self::set_view`] each draw; the screen owns no turns.
    view: InteractionView,
    /// True once the quit teardown finished (#949). Screen-local — the session
    /// is `Killed` by then. Drives the terminal banner and any-key auto-nav.
    terminated: bool,
    /// Why the session ended, set when the user confirms `Ctrl+W`.
    close_reason: Option<CloseReason>,
    /// True while the `Ctrl+W` confirm modal is visible.
    quit_modal_open: bool,
    /// Wall-clock start of the in-flight turn (from `TurnStarted`). Used to
    /// compute the elapsed-ms figure in the per-turn activity-log line.
    stream_started_at: Option<chrono::DateTime<Utc>>,
    /// Chunk counter for the in-flight turn (reset on each send).
    stream_chunks: usize,
    /// Number of turns sent this session — the `K` in "turn K".
    turn_count: usize,
    /// Animation tick for the braille "agent responding" spinner (#738 QA).
    spinner_tick: usize,
    /// Provider/CLI the turns spawn against (e.g. "claude"). Header display.
    agent_label: String,
    /// Model passed to each turn (e.g. "opus"). Header display.
    model: String,
    /// Issue title shown in the header so the frame names the work, like the
    /// sessions view does (#738 QA).
    issue_title: String,
    editor: TextArea<'static>,
    /// First visible history line. Meaningful only when `auto_scroll` is off.
    scroll_offset: usize,
    /// When true, the history pane re-pins to the bottom on every draw.
    /// Flips off the moment the user scrolls up; flips back on when they
    /// scroll to the bottom.
    auto_scroll: bool,
    /// Max scroll offset (`total - viewport`) at the last draw. Lets
    /// `scroll_down` clamp/re-pin without recomputing the viewport math.
    last_max_offset: usize,
    /// History viewport height (rows) at the last draw. Drives PageUp/PageDown
    /// paging math (#988).
    last_viewport: usize,
    /// Branch backing this session's worktree. Passed to teardown (#741).
    branch: String,
    /// Root the worktree lives under (`worktree_path`'s parent). Gates the
    /// destructive teardown to a safe location (#740 sanity check).
    worktree_root: PathBuf,

    /// When the screen entered `Terminated`. Drives the 500ms auto-nav timer.
    terminated_at: Option<Instant>,
    /// True while the quit teardown runs off-thread (#941/#949). Drives
    /// the "wiping worktree…" banner and the input lock.
    teardown_in_flight: bool,
    /// Teardown work parked for the app layer to run under `spawn_blocking`
    /// (#941). Set by the terminator path; taken via
    /// [`Self::take_pending_teardown_dispatch`].
    pending_teardown_dispatch: Option<lifecycle::TeardownDispatch>,
    /// Time source for the auto-nav timer. `RealClock` in production; a fake
    /// in tests.
    clock: Box<dyn Clock>,
    /// Read-only diff reviewer overlay (#918). `Some` while open; all input
    /// routes to it first, like the quit modal.
    diff_review: Option<diff_review::DiffReview>,
}

impl InteractionScreen {
    /// Construct an empty screen (no turns). Live sessions bind via
    /// [`Self::for_managed`].
    pub fn new() -> Self {
        Self::with_history(Vec::new())
    }

    /// Construct a screen pre-seeded with `turns` — test/snapshot seam that
    /// injects a transcript into the view without a live session (#950).
    pub fn with_history(turns: Vec<TurnRecord>) -> Self {
        let mut editor = TextArea::default();
        editor.set_cursor_line_style(Style::default());
        Self {
            issue_number: 0,
            produce_pr: true,
            worktree_path: PathBuf::new(),
            view: InteractionView {
                turns,
                ..InteractionView::default()
            },
            terminated: false,
            close_reason: None,
            quit_modal_open: false,
            stream_started_at: None,
            stream_chunks: 0,
            turn_count: 0,
            spinner_tick: 0,
            agent_label: String::new(),
            model: String::new(),
            issue_title: String::new(),
            editor,
            scroll_offset: 0,
            auto_scroll: true,
            last_max_offset: 0,
            last_viewport: 0,
            branch: String::new(),
            worktree_root: PathBuf::new(),
            terminated_at: None,
            teardown_in_flight: false,
            pending_teardown_dispatch: None,
            clock: Box::new(RealClock),
            diff_review: None,
        }
    }

    /// Bind a live unified interactive session (#948): copy its issue, launch
    /// flags, worktree, and a starting view projection. Used by the dispatch
    /// launch + re-entry paths (#738). The pool's `Session` stays the source
    /// of truth; the app refreshes the view each frame (#950).
    pub fn for_managed(managed: &crate::session::manager::ManagedSession) -> Self {
        let session = &managed.session;
        let mut screen = Self::new();
        screen.view = InteractionView::from_session(session);
        screen.issue_number = session.issue_number.unwrap_or_default();
        screen.produce_pr = session.produce_pr;
        screen.worktree_path = managed
            .worktree_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        screen.branch = managed.branch_name.clone().unwrap_or_default();
        // The worktree lives at `<root>/issue-N`, so its parent is the root the
        // teardown sanity-check gates against (#741 D1). Only an ABSOLUTE parent
        // is trusted: the pool's cwd-fallback sets `worktree_path = "."`, whose
        // parent is `""` — leaving the root empty makes `fire_terminator` skip
        // the destructive teardown instead of operating on the main repo.
        screen.worktree_root = screen
            .worktree_path
            .parent()
            .filter(|p| p.is_absolute())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        screen
    }

    /// True while a turn streams — the input pane is locked. Read from the
    /// live session's `turn_state` via the injected view (#950).
    pub fn is_streaming(&self) -> bool {
        self.view.turn_state == TurnState::Streaming
    }

    /// Project the live session into the rendered view (#950). Called each
    /// frame before draw; not called once the session is `Killed`, so the
    /// post-quit view freezes for the terminal banner.
    pub(crate) fn set_view(&mut self, view: InteractionView) {
        self.view = view;
    }

    /// Status-banner text from `settled_from` + `pr_linked` (#950).
    pub(crate) fn banner(&self) -> Option<String> {
        self.view.banner()
    }

    /// Issue this screen is bound to — lets the app fetch the live session to
    /// project into the view each frame (#950).
    pub(crate) fn issue_number(&self) -> u64 {
        self.issue_number
    }

    /// True when this screen is bound to `issue_number`. Gates the terminator
    /// bridge so a marker only drives the screen actually showing that issue.
    pub(crate) fn is_for_issue(&self, issue_number: u64) -> bool {
        self.issue_number == issue_number
    }

    /// True when the `Ctrl+P` pushup chord is active (launched with
    /// `produce_pr`, idle, and not terminated). Drives the greyed footer hint.
    pub fn pushup_enabled(&self) -> bool {
        self.produce_pr && self.view.turn_state == TurnState::Idle && !self.terminated
    }

    /// Current editor contents joined into one string. Seam for #738's submit.
    pub fn editor_text(&self) -> String {
        self.editor.lines().join("\n")
    }

    /// Open the diff reviewer overlay with the freshly computed diff (#918).
    /// Session state is untouched — the overlay is a pure view.
    pub(crate) fn open_diff_review(&mut self, diff_text: &str) {
        self.diff_review = Some(diff_review::DiffReview::new(diff_text));
    }
}

impl Default for InteractionScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for InteractionScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        if let Some(review) = self.diff_review.as_mut() {
            return match review.handle_key(*code, *modifiers) {
                diff_review::DiffReviewOutcome::Handled => ScreenAction::None,
                diff_review::DiffReviewOutcome::Close => {
                    self.diff_review = None;
                    ScreenAction::None
                }
                diff_review::DiffReviewOutcome::OpenShell => ScreenAction::OpenWorktreeShell {
                    worktree_path: self.worktree_path.clone(),
                },
            };
        }

        // Quit teardown in flight (#949): the session is already terminated
        // and git is wiping the worktree off-thread — swallow input until
        // the result lands (Terminated banner + auto-nav take over).
        if self.is_teardown_in_flight() {
            return ScreenAction::None;
        }

        if self.quit_modal_open {
            return self.handle_quit_modal(*code);
        }

        match classify(
            self.view.turn_state,
            self.terminated,
            self.produce_pr,
            *code,
            *modifiers,
        ) {
            InteractionIntent::Back => ScreenAction::Pop,
            InteractionIntent::ScrollUp => {
                self.scroll_up(1);
                ScreenAction::None
            }
            InteractionIntent::ScrollDown => {
                self.scroll_down(1);
                ScreenAction::None
            }
            InteractionIntent::PageUp => {
                self.page_up();
                ScreenAction::None
            }
            InteractionIntent::PageDown => {
                self.page_down();
                ScreenAction::None
            }
            InteractionIntent::JumpToLatest => {
                self.jump_to_latest();
                ScreenAction::None
            }
            InteractionIntent::InsertNewline => {
                self.editor.insert_newline();
                ScreenAction::None
            }
            InteractionIntent::ClearInput => {
                self.clear_editor();
                ScreenAction::None
            }
            InteractionIntent::SendInput => {
                let text = self.editor_text();
                if text.trim().is_empty() {
                    return ScreenAction::None;
                }
                self.clear_editor();
                self.begin_turn(text)
            }
            InteractionIntent::SendPushup => self.begin_turn(pushup_prompt(self.issue_number)),
            InteractionIntent::PushupDisabled => ScreenAction::LogActivity {
                tag: LOG_TAG.to_string(),
                message: format!(
                    "Ctrl+P disabled for #{} — session launched without Produce PR",
                    self.issue_number
                ),
                level: LogLevel::Info,
            },
            InteractionIntent::RequestQuit => {
                self.quit_modal_open = true;
                ScreenAction::None
            }
            InteractionIntent::OpenDiffReview => {
                // Greyed without an isolated worktree (post-teardown or
                // cwd-fallback) — there is nothing PR-equivalent to diff.
                if self.worktree_root.as_os_str().is_empty() {
                    return ScreenAction::LogActivity {
                        tag: LOG_TAG.to_string(),
                        message: format!(
                            "Ctrl+D unavailable for #{} — no isolated worktree to diff",
                            self.issue_number
                        ),
                        level: LogLevel::Info,
                    };
                }
                ScreenAction::OpenInteractionDiff {
                    worktree_path: self.worktree_path.clone(),
                }
            }
            InteractionIntent::FeedEditor => {
                self.editor.input(event.clone());
                ScreenAction::None
            }
            InteractionIntent::Locked => ScreenAction::None,
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Insert)
    }
}
