use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::theme::Theme;

use super::WidgetAction;

pub struct TextInput {
    pub label: String,
    pub value: String,
    pub cursor_position: usize,
    pub editing: bool,
}

impl TextInput {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor_position = value.len();
        Self {
            label: label.into(),
            value,
            cursor_position,
            editing: false,
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        if !self.editing {
            return match key.code {
                KeyCode::Enter => {
                    self.editing = true;
                    WidgetAction::RequestInsertMode
                }
                _ => WidgetAction::None,
            };
        }

        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.editing = false;
                WidgetAction::RequestNormalMode
            }
            KeyCode::Char(c) => {
                self.value.insert(self.cursor_position, c);
                self.cursor_position += 1;
                WidgetAction::Changed
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.value.remove(self.cursor_position);
                    WidgetAction::Changed
                } else {
                    WidgetAction::None
                }
            }
            KeyCode::Delete => {
                if self.cursor_position < self.value.len() {
                    self.value.remove(self.cursor_position);
                    WidgetAction::Changed
                } else {
                    WidgetAction::None
                }
            }
            KeyCode::Left => {
                self.cursor_position = self.cursor_position.saturating_sub(1);
                WidgetAction::None
            }
            KeyCode::Right => {
                if self.cursor_position < self.value.len() {
                    self.cursor_position += 1;
                }
                WidgetAction::None
            }
            KeyCode::Home => {
                self.cursor_position = 0;
                WidgetAction::None
            }
            KeyCode::End => {
                self.cursor_position = self.value.len();
                WidgetAction::None
            }
            _ => WidgetAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        let label_style = if focused {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        if self.editing {
            let (before, after) = self.value.split_at(self.cursor_position);
            let cursor_char = after.chars().next().unwrap_or(' ');
            let rest = if after.len() > 1 {
                &after[cursor_char.len_utf8()..]
            } else {
                ""
            };

            let line = Line::from(vec![
                Span::styled(format!("{}: ", self.label), label_style),
                Span::styled(before, Style::default().fg(theme.accent_success)),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::REVERSED),
                ),
                Span::styled(rest, Style::default().fg(theme.accent_success)),
            ]);
            f.render_widget(Paragraph::new(line), area);
        } else {
            let value_style = if focused {
                Style::default().fg(theme.text_primary)
            } else {
                Style::default().fg(theme.text_secondary)
            };
            let line = Line::from(vec![
                Span::styled(format!("{}: ", self.label), label_style),
                Span::styled(&self.value, value_style),
            ]);
            f.render_widget(Paragraph::new(line), area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn enter_starts_editing() {
        let mut w = TextInput::new("name", "hello");
        assert!(!w.editing);
        let action = w.handle_input(key(KeyCode::Enter));
        assert!(w.editing);
        assert_eq!(action, WidgetAction::RequestInsertMode);
    }

    #[test]
    fn esc_stops_editing() {
        let mut w = TextInput::new("name", "hello");
        w.handle_input(key(KeyCode::Enter)); // start editing
        let action = w.handle_input(key(KeyCode::Esc));
        assert!(!w.editing);
        assert_eq!(action, WidgetAction::RequestNormalMode);
    }

    #[test]
    fn char_inserts_at_cursor() {
        let mut w = TextInput::new("name", "");
        w.handle_input(key(KeyCode::Enter));
        w.handle_input(key(KeyCode::Char('a')));
        w.handle_input(key(KeyCode::Char('b')));
        assert_eq!(w.value, "ab");
        assert_eq!(w.cursor_position, 2);
    }

    #[test]
    fn backspace_removes_before_cursor() {
        let mut w = TextInput::new("name", "abc");
        w.handle_input(key(KeyCode::Enter));
        let action = w.handle_input(key(KeyCode::Backspace));
        assert_eq!(w.value, "ab");
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut w = TextInput::new("name", "abc");
        w.cursor_position = 0;
        w.editing = true;
        let action = w.handle_input(key(KeyCode::Backspace));
        assert_eq!(w.value, "abc");
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn cursor_movement_left_right() {
        let mut w = TextInput::new("name", "abc");
        w.editing = true;
        assert_eq!(w.cursor_position, 3);
        w.handle_input(key(KeyCode::Left));
        assert_eq!(w.cursor_position, 2);
        w.handle_input(key(KeyCode::Right));
        assert_eq!(w.cursor_position, 3);
    }

    #[test]
    fn cursor_clamps_at_boundaries() {
        let mut w = TextInput::new("name", "ab");
        w.editing = true;
        w.cursor_position = 0;
        w.handle_input(key(KeyCode::Left));
        assert_eq!(w.cursor_position, 0);

        w.cursor_position = 2;
        w.handle_input(key(KeyCode::Right));
        assert_eq!(w.cursor_position, 2);
    }

    #[test]
    fn home_end_keys() {
        let mut w = TextInput::new("name", "hello");
        w.editing = true;
        w.handle_input(key(KeyCode::Home));
        assert_eq!(w.cursor_position, 0);
        w.handle_input(key(KeyCode::End));
        assert_eq!(w.cursor_position, 5);
    }

    #[test]
    fn non_editing_ignores_chars() {
        let mut w = TextInput::new("name", "hello");
        let action = w.handle_input(key(KeyCode::Char('x')));
        assert_eq!(w.value, "hello");
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn delete_key_removes_at_cursor() {
        let mut w = TextInput::new("name", "abc");
        w.editing = true;
        w.cursor_position = 1;
        let action = w.handle_input(key(KeyCode::Delete));
        assert_eq!(w.value, "ac");
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn empty_string_operations() {
        let mut w = TextInput::new("name", "");
        w.editing = true;
        assert_eq!(w.handle_input(key(KeyCode::Backspace)), WidgetAction::None);
        assert_eq!(w.handle_input(key(KeyCode::Delete)), WidgetAction::None);
        assert_eq!(w.handle_input(key(KeyCode::Left)), WidgetAction::None);
    }
}
