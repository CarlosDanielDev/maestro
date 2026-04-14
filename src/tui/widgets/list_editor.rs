use crate::tui::icons::{self, IconId};
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

pub struct ListEditor {
    pub label: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub editing: bool,
    pub input_buffer: String,
    pub cursor_position: usize,
}

impl ListEditor {
    pub fn new(label: impl Into<String>, items: Vec<String>) -> Self {
        Self {
            label: label.into(),
            items,
            selected: 0,
            editing: false,
            input_buffer: String::new(),
            cursor_position: 0,
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        if self.editing {
            return self.handle_editing_input(key);
        }
        self.handle_normal_input(key)
    }

    fn handle_editing_input(&mut self, key: KeyEvent) -> WidgetAction {
        match key.code {
            KeyCode::Esc => {
                self.editing = false;
                self.input_buffer.clear();
                self.cursor_position = 0;
                WidgetAction::RequestNormalMode
            }
            KeyCode::Enter => {
                let trimmed = self.input_buffer.trim().to_string();
                self.editing = false;
                self.input_buffer.clear();
                self.cursor_position = 0;
                if !trimmed.is_empty() {
                    self.items.push(trimmed);
                    self.selected = self.items.len() - 1;
                    WidgetAction::Changed
                } else {
                    WidgetAction::RequestNormalMode
                }
            }
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
                WidgetAction::None
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
                WidgetAction::None
            }
            _ => WidgetAction::None,
        }
    }

    fn handle_normal_input(&mut self, key: KeyEvent) -> WidgetAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                WidgetAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.items.is_empty() && self.selected + 1 < self.items.len() {
                    self.selected += 1;
                }
                WidgetAction::None
            }
            KeyCode::Char('a') | KeyCode::Enter => {
                self.editing = true;
                self.input_buffer.clear();
                self.cursor_position = 0;
                WidgetAction::RequestInsertMode
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if !self.items.is_empty() {
                    self.items.remove(self.selected);
                    if self.selected >= self.items.len() && self.selected > 0 {
                        self.selected -= 1;
                    }
                    WidgetAction::Changed
                } else {
                    WidgetAction::None
                }
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

        let mut lines = vec![Line::from(Span::styled(
            format!("{}:", self.label),
            label_style,
        ))];

        for (i, item) in self.items.iter().enumerate() {
            let is_selected = i == self.selected && focused;
            let prefix = if is_selected {
                format!("{} ", icons::get(IconId::Selector))
            } else {
                "  ".to_string()
            };
            let style = if is_selected {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, item),
                style,
            )));
        }

        if self.editing {
            lines.push(Line::from(vec![
                Span::styled("+ ", Style::default().fg(theme.accent_success)),
                Span::raw(&self.input_buffer),
                Span::styled(
                    "_",
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::REVERSED),
                ),
            ]));
        } else if focused {
            lines.push(Line::from(Span::styled(
                "  [a] Add  [d] Delete",
                Style::default().fg(theme.text_muted),
            )));
        }

        f.render_widget(Paragraph::new(lines), area);
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
    fn add_item_flow() {
        let mut l = ListEditor::new("tags", vec![]);
        l.handle_input(key(KeyCode::Char('a')));
        assert!(l.editing);
        l.handle_input(key(KeyCode::Char('n')));
        l.handle_input(key(KeyCode::Char('e')));
        l.handle_input(key(KeyCode::Char('w')));
        let action = l.handle_input(key(KeyCode::Enter));
        assert_eq!(action, WidgetAction::Changed);
        assert_eq!(l.items, vec!["new"]);
        assert!(!l.editing);
    }

    #[test]
    fn cancel_add_with_esc() {
        let mut l = ListEditor::new("tags", vec!["existing".into()]);
        l.handle_input(key(KeyCode::Char('a')));
        l.handle_input(key(KeyCode::Char('x')));
        let action = l.handle_input(key(KeyCode::Esc));
        assert_eq!(action, WidgetAction::RequestNormalMode);
        assert_eq!(l.items, vec!["existing"]);
        assert!(!l.editing);
    }

    #[test]
    fn empty_input_not_added() {
        let mut l = ListEditor::new("tags", vec![]);
        l.handle_input(key(KeyCode::Char('a')));
        l.handle_input(key(KeyCode::Enter));
        assert!(l.items.is_empty());
    }

    #[test]
    fn delete_selected() {
        let mut l = ListEditor::new("tags", vec!["a".into(), "b".into(), "c".into()]);
        l.selected = 1;
        let action = l.handle_input(key(KeyCode::Char('d')));
        assert_eq!(action, WidgetAction::Changed);
        assert_eq!(l.items, vec!["a", "c"]);
    }

    #[test]
    fn delete_last_adjusts_selection() {
        let mut l = ListEditor::new("tags", vec!["a".into(), "b".into()]);
        l.selected = 1;
        l.handle_input(key(KeyCode::Char('d')));
        assert_eq!(l.selected, 0);
        assert_eq!(l.items, vec!["a"]);
    }

    #[test]
    fn delete_empty_list_no_panic() {
        let mut l = ListEditor::new("tags", vec![]);
        let action = l.handle_input(key(KeyCode::Char('d')));
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn navigate_with_jk() {
        let mut l = ListEditor::new("tags", vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(l.selected, 0);
        l.handle_input(key(KeyCode::Char('j')));
        assert_eq!(l.selected, 1);
        l.handle_input(key(KeyCode::Char('k')));
        assert_eq!(l.selected, 0);
    }

    #[test]
    fn navigation_clamps_at_boundaries() {
        let mut l = ListEditor::new("tags", vec!["a".into(), "b".into()]);
        l.handle_input(key(KeyCode::Char('k'))); // at 0, can't go up
        assert_eq!(l.selected, 0);
        l.selected = 1;
        l.handle_input(key(KeyCode::Char('j'))); // at last, can't go down
        assert_eq!(l.selected, 1);
    }

    #[test]
    fn backspace_in_editing_mode() {
        let mut l = ListEditor::new("tags", vec![]);
        l.handle_input(key(KeyCode::Char('a')));
        l.handle_input(key(KeyCode::Char('x')));
        l.handle_input(key(KeyCode::Char('y')));
        l.handle_input(key(KeyCode::Backspace));
        assert_eq!(l.input_buffer, "x");
    }
}
