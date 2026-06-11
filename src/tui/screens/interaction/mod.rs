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

use super::{Screen, ScreenAction};
use crate::session::interaction::{CloseReason, InteractionSession, InteractionState, TurnRecord};
use crate::session::interaction_lifecycle::InteractionLifecycleEvent;
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
    /// Lifecycle state mirrored from the domain `InteractionSession`. Drives
    /// the input lock (`Streaming`) and the terminal banner (`Terminated`).
    state: InteractionState,
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
    history: Vec<TurnRecord>,
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
    /// A terminator that arrived mid-stream; fired once the turn settles to
    /// `Idle` (#741 deferral). `None` in the common immediate path.
    queued_terminator: Option<InteractionLifecycleEvent>,
    /// When the screen entered `Terminated`. Drives the 500ms auto-nav timer.
    terminated_at: Option<Instant>,
    /// PR number of the teardown currently running off-thread (#941). `Some`
    /// from dispatch until `apply_teardown_result`; drives the "wiping
    /// worktree…" banner.
    teardown_pr_in_flight: Option<u64>,
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
    /// Construct an empty screen (no turns). Used by the dispatch construct
    /// arm in #736; live sessions bind via [`Self::for_session`].
    pub fn new() -> Self {
        Self::with_history(Vec::new())
    }

    /// Construct a screen pre-seeded with `history`. The seam #737 and the
    /// snapshot tests use to inject turns.
    pub fn with_history(history: Vec<TurnRecord>) -> Self {
        let mut editor = TextArea::default();
        editor.set_cursor_line_style(Style::default());
        Self {
            issue_number: 0,
            produce_pr: true,
            worktree_path: PathBuf::new(),
            state: InteractionState::Idle,
            close_reason: None,
            quit_modal_open: false,
            stream_started_at: None,
            stream_chunks: 0,
            turn_count: 0,
            spinner_tick: 0,
            agent_label: String::new(),
            model: String::new(),
            issue_title: String::new(),
            history,
            editor,
            scroll_offset: 0,
            auto_scroll: true,
            last_max_offset: 0,
            last_viewport: 0,
            branch: String::new(),
            worktree_root: PathBuf::new(),
            queued_terminator: None,
            terminated_at: None,
            teardown_pr_in_flight: None,
            pending_teardown_dispatch: None,
            clock: Box::new(RealClock),
            diff_review: None,
        }
    }

    /// Bind a live `InteractionSession`: copy its issue, launch flags,
    /// worktree, state, and a snapshot of its history. Used by the dispatch
    /// launch + re-entry paths (#738). The screen is a view; the pool's
    /// session remains the persistence source of truth.
    pub fn for_session(session: &InteractionSession) -> Self {
        let mut screen = Self::with_history(session.history.clone());
        screen.issue_number = session.issue_number;
        screen.produce_pr = session.produce_pr;
        screen.worktree_path = session.worktree_path.clone();
        screen.state = session.state;
        screen.close_reason = session.close_reason.clone();
        screen.branch = session.branch.clone();
        // The worktree lives at `<root>/issue-N`, so its parent is the root the
        // teardown sanity-check gates against (#741 D1). Only an ABSOLUTE parent
        // is trusted: the pool's cwd-fallback sets `worktree_path = "."`, whose
        // parent is `""` — leaving the root empty makes `fire_terminator` skip
        // the destructive teardown instead of operating on the main repo.
        screen.worktree_root = session
            .worktree_path
            .parent()
            .filter(|p| p.is_absolute())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        screen
    }

    /// True while a turn streams — the input pane is locked.
    pub fn is_streaming(&self) -> bool {
        self.state == InteractionState::Streaming
    }

    /// True when this screen is bound to `issue_number`. Gates the terminator
    /// bridge so a marker only drives the screen actually showing that issue.
    pub(crate) fn is_for_issue(&self, issue_number: u64) -> bool {
        self.issue_number == issue_number
    }

    /// True when the `Ctrl+P` pushup chord is active (launched with
    /// `produce_pr` and not mid-stream). Drives the greyed footer hint.
    pub fn pushup_enabled(&self) -> bool {
        self.produce_pr && self.state == InteractionState::Idle
    }

    /// Append a turn. Preserves the user's read position: when scrolled up
    /// (`auto_scroll` off), the offset is left untouched so incoming turns
    /// don't yank the viewport.
    pub fn push_turn(&mut self, turn: TurnRecord) {
        self.history.push(turn);
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

    /// Scroll the open reviewer for snapshot tests / mouse routing.
    #[cfg(test)]
    pub(crate) fn diff_review_open(&self) -> bool {
        self.diff_review.is_some()
    }

    #[cfg(test)]
    pub(crate) fn scroll_up_for_test(&mut self, n: usize) {
        self.scroll_up(n);
    }

    /// History length — test seam for dispatch re-entry assertions (#738).
    #[cfg(test)]
    pub(crate) fn history_len(&self) -> usize {
        self.history.len()
    }

    #[cfg(test)]
    fn scroll_down_for_test(&mut self, n: usize) {
        self.scroll_down(n);
    }

    /// Tail-follow flag — cross-module test seam for the mouse-routing
    /// assertion in `tui::mod` (#988).
    #[cfg(test)]
    pub(crate) fn auto_scroll_for_test(&self) -> bool {
        self.auto_scroll
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

        if self.quit_modal_open {
            return self.handle_quit_modal(*code);
        }

        match classify(self.state, self.produce_pr, *code, *modifiers) {
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
