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

pub struct Dropdown {
    pub label: String,
    pub options: Vec<String>,
    pub selected: usize,
}

impl Dropdown {
    pub fn new(label: impl Into<String>, options: Vec<String>, selected: usize) -> Self {
        Self {
            label: label.into(),
            options,
            selected,
        }
    }

    pub fn selected_value(&self) -> &str {
        &self.options[self.selected]
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        if self.options.is_empty() {
            return WidgetAction::None;
        }

        match key.code {
            KeyCode::Right | KeyCode::Char('l') => {
                let new = (self.selected + 1) % self.options.len();
                if new != self.selected {
                    self.selected = new;
                    WidgetAction::Changed
                } else {
                    WidgetAction::None
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let new = if self.selected == 0 {
                    self.options.len() - 1
                } else {
                    self.selected - 1
                };
                if new != self.selected {
                    self.selected = new;
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

        let value = self.options.get(self.selected).map_or("", |s| s.as_str());
        let value_style = if focused {
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_secondary)
        };

        let arrow_color = if focused {
            theme.accent_success
        } else {
            theme.text_muted
        };

        let line = Line::from(vec![
            Span::styled(format!("{}: ", self.label), label_style),
            Span::styled("< ", Style::default().fg(arrow_color)),
            Span::styled(value, value_style),
            Span::styled(" >", Style::default().fg(arrow_color)),
        ]);
        f.render_widget(Paragraph::new(line), area);
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

    fn options() -> Vec<String> {
        vec!["one".into(), "two".into(), "three".into()]
    }

    #[test]
    fn cycles_right() {
        let mut d = Dropdown::new("choice", options(), 0);
        d.handle_input(key(KeyCode::Right));
        assert_eq!(d.selected, 1);
        d.handle_input(key(KeyCode::Right));
        assert_eq!(d.selected, 2);
    }

    #[test]
    fn wraps_right_to_first() {
        let mut d = Dropdown::new("choice", options(), 2);
        d.handle_input(key(KeyCode::Right));
        assert_eq!(d.selected, 0);
    }

    #[test]
    fn wraps_left_to_last() {
        let mut d = Dropdown::new("choice", options(), 0);
        d.handle_input(key(KeyCode::Left));
        assert_eq!(d.selected, 2);
    }

    #[test]
    fn selected_value_returns_current() {
        let d = Dropdown::new("choice", options(), 1);
        assert_eq!(d.selected_value(), "two");
    }

    #[test]
    fn single_option_no_change() {
        let mut d = Dropdown::new("choice", vec!["only".into()], 0);
        let action = d.handle_input(key(KeyCode::Right));
        assert_eq!(d.selected, 0);
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn empty_options_no_panic() {
        let mut d = Dropdown::new("choice", vec![], 0);
        let action = d.handle_input(key(KeyCode::Right));
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn vim_keys() {
        let mut d = Dropdown::new("choice", options(), 0);
        d.handle_input(key(KeyCode::Char('l')));
        assert_eq!(d.selected, 1);
        d.handle_input(key(KeyCode::Char('h')));
        assert_eq!(d.selected, 0);
    }
}
