mod draw;

use super::{Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::text::Text;
use ratatui::{Frame, layout::Rect};

pub struct ReleaseNotesScreen {
    pub(super) scroll_offset: u16,
    pub(super) cached_content: Option<Text<'static>>,
    pub(super) total_lines: u16,
}

impl ReleaseNotesScreen {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            cached_content: None,
            total_lines: 0,
        }
    }

    fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(lines)
            .min(self.total_lines.saturating_sub(1));
    }

    fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.total_lines.saturating_sub(1);
    }
}

impl KeymapProvider for ReleaseNotesScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Navigation",
                bindings: vec![
                    KeyBinding {
                        key: "j/Down",
                        description: "Scroll down",
                    },
                    KeyBinding {
                        key: "k/Up",
                        description: "Scroll up",
                    },
                    KeyBinding {
                        key: "PgDn",
                        description: "Page down",
                    },
                    KeyBinding {
                        key: "PgUp",
                        description: "Page up",
                    },
                    KeyBinding {
                        key: "Home",
                        description: "Go to top",
                    },
                    KeyBinding {
                        key: "End",
                        description: "Go to bottom",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Actions",
                bindings: vec![KeyBinding {
                    key: "Esc",
                    description: "Back",
                }],
            },
        ]
    }
}

impl Screen for ReleaseNotesScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Char('j') | KeyCode::Down => self.scroll_down(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll_up(1),
                KeyCode::PageDown => self.scroll_down(10),
                KeyCode::PageUp => self.scroll_up(10),
                KeyCode::Home => self.scroll_offset = 0,
                KeyCode::End => self.scroll_to_bottom(),
                KeyCode::Esc => return ScreenAction::Pop,
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        draw::draw_release_notes(self, f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;

    #[test]
    fn new_defaults() {
        let screen = ReleaseNotesScreen::new();
        assert_eq!(screen.scroll_offset, 0);
        assert!(screen.cached_content.is_none());
        assert_eq!(screen.total_lines, 0);
    }

    #[test]
    fn scroll_down_increments_offset() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        screen.scroll_down(1);
        assert_eq!(screen.scroll_offset, 1);
    }

    #[test]
    fn scroll_up_decrements_offset() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        screen.scroll_offset = 5;
        screen.scroll_up(1);
        assert_eq!(screen.scroll_offset, 4);
    }

    #[test]
    fn scroll_up_at_zero_stays_at_zero() {
        let mut screen = ReleaseNotesScreen::new();
        screen.scroll_up(1);
        assert_eq!(screen.scroll_offset, 0);
    }

    #[test]
    fn scroll_down_clamps_to_total_lines() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 10;
        screen.scroll_down(20);
        assert_eq!(screen.scroll_offset, 9);
    }

    #[test]
    fn page_down_scrolls_by_10() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        screen.scroll_down(10);
        assert_eq!(screen.scroll_offset, 10);
    }

    #[test]
    fn page_up_scrolls_by_10() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        screen.scroll_offset = 20;
        screen.scroll_up(10);
        assert_eq!(screen.scroll_offset, 10);
    }

    #[test]
    fn home_resets_to_zero() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        screen.scroll_offset = 50;
        screen.handle_input(&key_event(KeyCode::Home), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 0);
    }

    #[test]
    fn end_scrolls_to_bottom() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        screen.scroll_to_bottom();
        assert_eq!(screen.scroll_offset, 99);
    }

    #[test]
    fn esc_returns_pop() {
        let mut screen = ReleaseNotesScreen::new();
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn j_returns_none() {
        let mut screen = ReleaseNotesScreen::new();
        screen.total_lines = 100;
        let action = screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn unknown_key_returns_none() {
        let mut screen = ReleaseNotesScreen::new();
        let action = screen.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn desired_input_mode_is_normal() {
        let screen = ReleaseNotesScreen::new();
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Normal));
    }

    #[test]
    fn keybindings_returns_two_groups() {
        let screen = ReleaseNotesScreen::new();
        let groups = screen.keybindings();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Navigation");
        assert_eq!(groups[1].title, "Actions");
    }
}
