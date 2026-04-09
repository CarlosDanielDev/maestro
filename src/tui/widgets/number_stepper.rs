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

pub struct NumberStepper {
    pub label: String,
    pub value: i64,
    pub min: i64,
    pub max: i64,
    pub step: i64,
}

impl NumberStepper {
    pub fn new(label: impl Into<String>, value: i64, min: i64, max: i64) -> Self {
        Self {
            label: label.into(),
            value,
            min,
            max,
            step: 1,
        }
    }

    pub fn with_step(mut self, step: i64) -> Self {
        self.step = step;
        self
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        match key.code {
            KeyCode::Right | KeyCode::Char('l') => {
                let new_value = (self.value + self.step).min(self.max);
                if new_value != self.value {
                    self.value = new_value;
                    WidgetAction::Changed
                } else {
                    WidgetAction::None
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let new_value = (self.value - self.step).max(self.min);
                if new_value != self.value {
                    self.value = new_value;
                    WidgetAction::Changed
                } else {
                    WidgetAction::None
                }
            }
            _ => WidgetAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        let label_style = Style::default().fg(if focused {
            theme.text_primary
        } else {
            theme.text_secondary
        });

        let left_dim = self.value <= self.min;
        let right_dim = self.value >= self.max;

        let left_arrow = if left_dim {
            Style::default().fg(theme.text_muted)
        } else {
            Style::default().fg(theme.accent_info)
        };
        let right_arrow = if right_dim {
            Style::default().fg(theme.text_muted)
        } else {
            Style::default().fg(theme.accent_info)
        };

        let value_style = if focused {
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_secondary)
        };

        let line = Line::from(vec![
            Span::styled(format!("{}: ", self.label), label_style),
            Span::styled("< ", left_arrow),
            Span::styled(self.value.to_string(), value_style),
            Span::styled(" >", right_arrow),
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
    fn increment_with_right() {
        let mut s = NumberStepper::new("count", 5, 0, 10);
        let action = s.handle_input(key(KeyCode::Right));
        assert_eq!(s.value, 6);
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn decrement_with_left() {
        let mut s = NumberStepper::new("count", 5, 0, 10);
        let action = s.handle_input(key(KeyCode::Left));
        assert_eq!(s.value, 4);
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn clamps_at_max() {
        let mut s = NumberStepper::new("count", 10, 0, 10);
        let action = s.handle_input(key(KeyCode::Right));
        assert_eq!(s.value, 10);
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn clamps_at_min() {
        let mut s = NumberStepper::new("count", 0, 0, 10);
        let action = s.handle_input(key(KeyCode::Left));
        assert_eq!(s.value, 0);
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn custom_step() {
        let mut s = NumberStepper::new("count", 0, 0, 100).with_step(10);
        s.handle_input(key(KeyCode::Right));
        assert_eq!(s.value, 10);
        s.handle_input(key(KeyCode::Right));
        assert_eq!(s.value, 20);
    }

    #[test]
    fn step_clamps_to_max() {
        let mut s = NumberStepper::new("count", 95, 0, 100).with_step(10);
        s.handle_input(key(KeyCode::Right));
        assert_eq!(s.value, 100);
    }

    #[test]
    fn vim_keys_work() {
        let mut s = NumberStepper::new("count", 5, 0, 10);
        s.handle_input(key(KeyCode::Char('l')));
        assert_eq!(s.value, 6);
        s.handle_input(key(KeyCode::Char('h')));
        assert_eq!(s.value, 5);
    }

    #[test]
    fn ignores_other_keys() {
        let mut s = NumberStepper::new("count", 5, 0, 10);
        let action = s.handle_input(key(KeyCode::Char('x')));
        assert_eq!(s.value, 5);
        assert_eq!(action, WidgetAction::None);
    }
}
