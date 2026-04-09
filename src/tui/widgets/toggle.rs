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

pub struct Toggle {
    pub label: String,
    pub value: bool,
}

impl Toggle {
    pub fn new(label: impl Into<String>, value: bool) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.value = !self.value;
                WidgetAction::Changed
            }
            _ => WidgetAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        let indicator = if self.value { "[x]" } else { "[ ]" };
        let check_color = if self.value {
            theme.accent_success
        } else {
            if focused {
                theme.text_primary
            } else {
                theme.text_muted
            }
        };
        let check_style = if focused {
            Style::default()
                .fg(check_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(check_color)
        };
        let label_style = if focused {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", indicator), check_style),
            Span::styled(&self.label, label_style),
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

    #[test]
    fn toggle_flips_on_space() {
        let mut t = Toggle::new("test", false);
        assert!(!t.value);
        let action = t.handle_input(key(KeyCode::Char(' ')));
        assert!(t.value);
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn toggle_flips_on_enter() {
        let mut t = Toggle::new("test", true);
        let action = t.handle_input(key(KeyCode::Enter));
        assert!(!t.value);
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn toggle_ignores_other_keys() {
        let mut t = Toggle::new("test", false);
        let action = t.handle_input(key(KeyCode::Char('x')));
        assert!(!t.value);
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn toggle_double_flip_returns_to_original() {
        let mut t = Toggle::new("test", false);
        t.handle_input(key(KeyCode::Char(' ')));
        t.handle_input(key(KeyCode::Char(' ')));
        assert!(!t.value);
    }
}
