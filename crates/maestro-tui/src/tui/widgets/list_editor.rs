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

use super::{WidgetAction, focused_selection_style};

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
            focused_selection_style(theme)
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
                focused_selection_style(theme)
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
            // Empty + focused gets a discoverable hint in accent_info
            // (instead of the muted [a] Add [d] Delete row used when
            // the list already has items proving it's editable). #900
            // surfaced the prior muted hint as essentially invisible on
            // freshly-added entries — the field looked broken.
            let (hint, style) = if self.items.is_empty() {
                (
                    "  (empty — [a] Add to start)",
                    Style::default()
                        .fg(theme.accent_info)
                        .add_modifier(Modifier::ITALIC),
                )
            } else {
                (
                    "  [a] Add  [d] Delete",
                    Style::default().fg(theme.text_muted),
                )
            };
            lines.push(Line::from(Span::styled(hint, style)));
        } else if self.items.is_empty() {
            // Unfocused + empty still needs a visible marker so the user
            // can tell the field is editable without having to focus it
            // first. Muted style matches the rest of the inert form chrome.
            lines.push(Line::from(Span::styled(
                "  (empty)",
                Style::default().fg(theme.text_muted),
            )));
        }

        f.render_widget(Paragraph::new(lines), area);
    }

    /// Test-only helper: returns the rendered lines (label + items +
    /// trailing hint/empty marker) without going through the ratatui
    /// `Frame`. Used by the empty-state regression tests for #900.
    #[cfg(test)]
    pub(crate) fn render_lines_for_test(&self, focused: bool) -> Vec<String> {
        let mut out = vec![format!("{}:", self.label)];
        for (i, item) in self.items.iter().enumerate() {
            let prefix = if i == self.selected && focused {
                format!("{} ", icons::get(IconId::Selector))
            } else {
                "  ".to_string()
            };
            out.push(format!("{prefix}{item}"));
        }
        if self.editing {
            out.push(format!("+ {}_", self.input_buffer));
        } else if focused {
            out.push(
                if self.items.is_empty() {
                    "  (empty — [a] Add to start)"
                } else {
                    "  [a] Add  [d] Delete"
                }
                .to_string(),
            );
        } else if self.items.is_empty() {
            out.push("  (empty)".to_string());
        }
        out
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
    fn empty_focused_renders_discoverable_hint() {
        // Regression for #900: an empty focused ListEditor must show
        // an obvious "(empty — [a] Add to start)" prompt so a fresh
        // settings entry doesn't look broken.
        let l = ListEditor::new("bindings", vec![]);
        let lines = l.render_lines_for_test(true);
        assert_eq!(lines.len(), 2, "label + empty hint");
        assert_eq!(lines[0], "bindings:");
        assert!(
            lines[1].contains("(empty"),
            "focused empty hint must mark the field as empty: {lines:?}"
        );
        assert!(
            lines[1].contains("[a] Add"),
            "focused empty hint must advertise [a] Add: {lines:?}"
        );
    }

    #[test]
    fn empty_unfocused_still_shows_empty_marker() {
        // Regression for #900: even when not focused, an empty list
        // needs a visible "(empty)" marker so the user knows the field
        // is editable without having to land focus on it first.
        let l = ListEditor::new("bindings", vec![]);
        let lines = l.render_lines_for_test(false);
        assert_eq!(lines.len(), 2, "label + (empty)");
        assert_eq!(lines[0], "bindings:");
        assert_eq!(lines[1].trim(), "(empty)");
    }

    #[test]
    fn non_empty_focused_renders_add_delete_hint_unchanged() {
        // Regression guard for #900: non-empty path keeps the original
        // [a] Add [d] Delete hint; only the empty branch changed.
        let l = ListEditor::new("bindings", vec!["coder=claude".into()]);
        let lines = l.render_lines_for_test(true);
        assert_eq!(lines.len(), 3, "label + 1 row + hint");
        assert!(
            lines.last().unwrap().contains("[a] Add"),
            "non-empty focused hint must still advertise Add: {lines:?}"
        );
        assert!(
            lines.last().unwrap().contains("[d] Delete"),
            "non-empty focused hint must still advertise Delete: {lines:?}"
        );
    }

    #[test]
    fn non_empty_unfocused_renders_no_trailing_hint() {
        // Regression guard for #900: when the list has items, an
        // unfocused widget should NOT add a trailing hint line —
        // existing snapshot consumers rely on this shape.
        let l = ListEditor::new("bindings", vec!["coder=claude".into()]);
        let lines = l.render_lines_for_test(false);
        assert_eq!(lines.len(), 2, "label + 1 row, no trailing hint");
        assert_eq!(lines[0], "bindings:");
        assert!(lines[1].contains("coder=claude"));
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
