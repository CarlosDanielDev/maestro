//! Interaction screen (#736) — chat-style transcript + multi-line input.
//!
//! UI scaffolding only. Binds to one `InteractionSession`'s history and
//! renders it. Per-turn agent spawning lands in #737; the rich keymap and
//! re-entry wiring land in #738. This screen sends no prompts.

mod history;
mod input;
#[cfg(test)]
mod tests;

use super::{Screen, ScreenAction};
use crate::session::interaction::TurnRecord;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
};
use tui_textarea::TextArea;

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
    /// arm in #736; #737 swaps in a constructor that binds live session data.
    pub fn new() -> Self {
        Self::with_history(Vec::new())
    }

    /// Construct a screen pre-seeded with `history`. The seam #737 and the
    /// snapshot tests use to inject turns.
    pub fn with_history(history: Vec<TurnRecord>) -> Self {
        let mut editor = TextArea::default();
        editor.set_cursor_line_style(Style::default());
        Self {
            history,
            editor,
            scroll_offset: 0,
            auto_scroll: true,
            last_max_offset: 0,
        }
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
        let chunks =
            Layout::vertical([Constraint::Min(0), Constraint::Length(INPUT_HEIGHT)]).split(area);
        let history_area = chunks[0];
        let input_area = chunks[1];

        let total = history::build_lines(&self.history, theme).len();
        let viewport = history_area.height as usize;
        self.last_max_offset = total.saturating_sub(viewport);
        let offset = effective_offset(self.auto_scroll, self.scroll_offset, total, viewport);
        if self.auto_scroll {
            self.scroll_offset = offset;
        }

        history::draw_history(f, history_area, theme, &self.history, offset);
        input::draw_input(f, input_area, theme, &self.editor);
    }

    #[cfg(test)]
    pub(crate) fn scroll_up_for_test(&mut self, n: usize) {
        self.scroll_up(n);
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
        vec![KeyBindingGroup {
            title: "Interaction",
            bindings: vec![
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
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Esc => return ScreenAction::Pop,
                KeyCode::Up => self.scroll_up(1),
                KeyCode::Down => self.scroll_down(1),
                _ => {
                    self.editor.input(event.clone());
                }
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Insert)
    }
}
