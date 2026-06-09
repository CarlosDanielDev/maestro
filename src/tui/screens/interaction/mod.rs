//! Interaction screen (#736) — chat-style transcript + multi-line input.
//!
//! UI scaffolding only. Binds to one `InteractionSession`'s history and
//! renders it. Per-turn agent spawning lands in #737; the rich keymap and
//! re-entry wiring land in #738. This screen sends no prompts.

mod history;
mod input;
mod keymap;
#[cfg(test)]
mod keymap_tests;
mod layout;
pub(crate) mod lifecycle;
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
use lifecycle::{Clock, RealClock, RealTeardown, WorktreeTeardownPort};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
};
use std::path::PathBuf;
use std::time::Instant;
use tui_textarea::TextArea;

/// Tag used for every Interaction activity-log line.
const LOG_TAG: &str = "INTERACTION";

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
    /// Destructive worktree teardown seam (#740). `RealTeardown` in
    /// production; a fake in tests.
    teardown: Box<dyn WorktreeTeardownPort>,
    /// Time source for the auto-nav timer. `RealClock` in production; a fake
    /// in tests.
    clock: Box<dyn Clock>,
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
            teardown: Box::new(RealTeardown),
            clock: Box::new(RealClock),
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

    /// Scroll the history up by `n` lines. Takes manual control of the
    /// viewport (disables tail-following). `pub(crate)` so the mouse-wheel
    /// routing in `tui::mod` can drive it (#988).
    pub(crate) fn scroll_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll the history down by `n` lines, clamped to the last-known
    /// bottom. Re-pins tail-following once the bottom is reached. `pub(crate)`
    /// for the mouse-wheel routing (#988).
    pub(crate) fn scroll_down(&mut self, n: usize) {
        let max = self.last_max_offset;
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max);
        if self.scroll_offset >= max {
            self.auto_scroll = true;
        }
    }

    /// Page the transcript up by one viewport height (#988). Clamps at the top
    /// via `scroll_up`'s `saturating_sub`.
    fn page_up(&mut self) {
        self.scroll_up(self.last_viewport.max(1));
    }

    /// Page the transcript down by one viewport height (#988). Clamps at
    /// `last_max_offset` and re-pins tail-following via `scroll_down`.
    fn page_down(&mut self) {
        self.scroll_down(self.last_viewport.max(1));
    }

    /// Jump to the newest message and resume tail-following (#988). The draw
    /// path recomputes the concrete offset from `auto_scroll`.
    fn jump_to_latest(&mut self) {
        self.auto_scroll = true;
        self.scroll_offset = self.last_max_offset;
    }

    fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::vertical([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(INPUT_HEIGHT),
        ])
        .split(area);
        let header_area = chunks[0];
        // Inset the transcript by one column each side so the rounded card
        // borders get a gutter and the right border never clips against the
        // terminal edge (#987 QA).
        let history_area = inset_x(chunks[1], 1);
        let keybar_area = chunks[2];
        let input_area = chunks[3];

        input::draw_header(
            f,
            header_area,
            theme,
            &self.agent_label,
            &self.model,
            self.issue_number,
            &self.issue_title,
        );

        let total = history::visual_total(&self.history, theme, history_area.width);
        let viewport = history_area.height as usize;
        self.last_max_offset = total.saturating_sub(viewport);
        self.last_viewport = viewport;
        let offset = effective_offset(self.auto_scroll, self.scroll_offset, total, viewport);
        if self.auto_scroll {
            self.scroll_offset = offset;
        }

        history::draw_history(
            f,
            history_area,
            theme,
            &self.history,
            offset,
            self.issue_number,
            &self.issue_title,
        );
        input::draw_keybar(f, keybar_area, theme, self.pushup_enabled());
        if self.state == InteractionState::Terminated {
            input::draw_terminated_banner(f, input_area, theme, self.close_reason.as_ref());
        } else {
            let spinner = crate::tui::spinner::graph_node_frame(
                self.spinner_tick,
                crate::icon_mode::use_nerd_font(),
            );
            input::draw_input(
                f,
                input_area,
                theme,
                &self.editor,
                self.is_streaming(),
                spinner,
            );
        }

        if self.quit_modal_open {
            input::draw_quit_modal(f, area, theme, &self.worktree_path);
        }
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

    /// Current lifecycle state — test seam for the live-turn integration (#738).
    #[cfg(test)]
    #[allow(dead_code)] // Reason: consumed by the bin-crate integration test interaction_turn_live
    pub(crate) fn history_state(&self) -> InteractionState {
        self.state
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
