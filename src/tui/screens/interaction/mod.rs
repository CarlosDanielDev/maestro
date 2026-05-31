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
#[cfg(test)]
mod tests;
mod turn;

use super::{Screen, ScreenAction};
use crate::session::interaction::{CloseReason, InteractionSession, InteractionState, TurnRecord};
use crate::tui::activity_log::LogLevel;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use chrono::Utc;
use crossterm::event::{Event, KeyEvent, KeyEventKind};
use keymap::{InteractionIntent, classify, pushup_prompt};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
};
use std::path::PathBuf;
use tui_textarea::TextArea;

/// Tag used for every Interaction activity-log line.
const LOG_TAG: &str = "INTERACTION";

/// Fixed height (rows, incl. borders) reserved for the input pane.
const INPUT_HEIGHT: u16 = 5;

/// Compute the vertical scroll offset to render at. When `auto_scroll` is
/// on, the pane follows the tail (returns the max offset). When off, it
/// honors the user's `scroll_offset`, clamped so it never scrolls past the
/// last line.
fn effective_offset(
    auto_scroll: bool,
    scroll_offset: usize,
    total: usize,
    viewport: usize,
) -> usize {
    let max = total.saturating_sub(viewport);
    if auto_scroll {
        max
    } else {
        scroll_offset.min(max)
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
    /// Why the session ended, set when the user confirms `Ctrl+Q`.
    close_reason: Option<CloseReason>,
    /// True while the `Ctrl+Q` confirm modal is visible.
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
            history,
            editor,
            scroll_offset: 0,
            auto_scroll: true,
            last_max_offset: 0,
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
        screen
    }

    /// Inject the global animation tick so the "agent responding" spinner
    /// advances each frame while streaming (#738 QA).
    pub fn set_spinner_context(&mut self, spinner_tick: usize) {
        self.spinner_tick = spinner_tick;
    }

    /// Record which provider/CLI and model the turns run against, shown in
    /// the screen header so the user knows who they are talking to (#738 QA).
    pub fn set_provider_context(
        &mut self,
        agent_label: impl Into<String>,
        model: impl Into<String>,
    ) {
        self.agent_label = agent_label.into();
        self.model = model.into();
    }

    /// True while a turn streams — the input pane is locked.
    pub fn is_streaming(&self) -> bool {
        self.state == InteractionState::Streaming
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
    /// viewport (disables tail-following).
    fn scroll_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll the history down by `n` lines, clamped to the last-known
    /// bottom. Re-pins tail-following once the bottom is reached.
    fn scroll_down(&mut self, n: usize) {
        let max = self.last_max_offset;
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max);
        if self.scroll_offset >= max {
            self.auto_scroll = true;
        }
    }

    fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(INPUT_HEIGHT),
        ])
        .split(area);
        let header_area = chunks[0];
        let history_area = chunks[1];
        let keybar_area = chunks[2];
        let input_area = chunks[3];

        input::draw_header(
            f,
            header_area,
            theme,
            &self.agent_label,
            &self.model,
            self.issue_number,
        );

        let total = history::build_lines(&self.history, theme).len();
        let viewport = history_area.height as usize;
        self.last_max_offset = total.saturating_sub(viewport);
        let offset = effective_offset(self.auto_scroll, self.scroll_offset, total, viewport);
        if self.auto_scroll {
            self.scroll_offset = offset;
        }

        history::draw_history(f, history_area, theme, &self.history, offset);
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
}

impl Default for InteractionScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapProvider for InteractionScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        let pushup = if self.pushup_enabled() {
            KeyBinding {
                key: "Ctrl+P",
                description: "Send /pushup",
            }
        } else {
            KeyBinding {
                key: "Ctrl+P",
                description: "Send /pushup (greyed: no Produce PR)",
            }
        };
        vec![KeyBindingGroup {
            title: "Interaction",
            bindings: vec![
                KeyBinding {
                    key: "Enter",
                    description: "Send",
                },
                KeyBinding {
                    key: "Shift+Enter",
                    description: "Newline",
                },
                pushup,
                KeyBinding {
                    key: "Ctrl+L",
                    description: "Clear input",
                },
                KeyBinding {
                    key: "Ctrl+Q",
                    description: "Quit",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Back",
                },
                KeyBinding {
                    key: "Up/Down",
                    description: "Scroll history",
                },
            ],
        }]
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
