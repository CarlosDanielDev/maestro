use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::screens::settings::validation::{ValidationFeedback, ValidationSeverity};
use crate::tui::theme::Theme;

use super::{WidgetAction, focused_selection_style};

pub struct NumberStepper {
    pub label: String,
    pub value: i64,
    pub min: i64,
    pub max: i64,
    pub step: i64,
    display_divisor: Option<u32>,
}

impl NumberStepper {
    pub fn new(label: impl Into<String>, value: i64, min: i64, max: i64) -> Self {
        Self {
            label: label.into(),
            value,
            min,
            max,
            step: 1,
            display_divisor: None,
        }
    }

    pub fn with_step(mut self, step: i64) -> Self {
        self.step = step;
        self
    }

    /// Render `value / divisor` as a fractional number. `divisor` should be
    /// a power of 10 (10 = one decimal, 100 = two decimals). Internal `i64`
    /// representation is unchanged — only the rendered string is affected.
    /// Passing `Some(1)` or `None` renders the raw integer.
    pub fn with_display_divisor(mut self, divisor: Option<u32>) -> Self {
        self.display_divisor = divisor;
        self
    }

    /// Format the stored integer for the user. Pure function — used by
    /// `draw` and exercised directly in tests.
    pub fn display_value(&self) -> String {
        match self.display_divisor {
            None => self.value.to_string(),
            Some(d) if d <= 1 => self.value.to_string(),
            Some(d) => {
                let decimals = (d as f64).log10().round() as usize;
                format!("{:.*}", decimals, self.value as f64 / d as f64)
            }
        }
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

    pub fn draw(
        &self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        focused: bool,
        validation: Option<&ValidationFeedback>,
    ) {
        let label_style = match validation.map(|v| v.severity) {
            Some(ValidationSeverity::Error) => Style::default()
                .fg(theme.accent_error)
                .add_modifier(Modifier::BOLD),
            Some(ValidationSeverity::Warning) => Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
            _ if focused => focused_selection_style(theme),
            _ => Style::default().fg(theme.text_primary),
        };

        let left_arrow = if focused {
            focused_selection_style(theme)
        } else {
            Style::default().fg(theme.text_muted)
        };
        let right_arrow = if focused {
            focused_selection_style(theme)
        } else {
            Style::default().fg(theme.text_muted)
        };

        let value_style = if focused {
            focused_selection_style(theme)
        } else {
            Style::default().fg(theme.text_secondary)
        };

        let line = Line::from(vec![
            Span::styled(format!("{}: ", self.label), label_style),
            Span::styled("< ", left_arrow),
            Span::styled(self.display_value(), value_style),
            Span::styled(" >", right_arrow),
        ]);
        f.render_widget(Paragraph::new(line), area);

        // Render inline error/warning message on the next line
        if let Some(fb) = validation
            && !fb.message.is_empty()
            && area.height > 1
        {
            let msg_area = Rect {
                y: area.y + 1,
                height: 1,
                ..area
            };
            let color = match fb.severity {
                ValidationSeverity::Error => theme.accent_error,
                ValidationSeverity::Warning => theme.accent_warning,
                ValidationSeverity::Valid => theme.text_muted,
            };
            let prefix_len = self.label.len() + 2;
            let padding = " ".repeat(prefix_len);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{}{}", padding, fb.message),
                    Style::default().fg(color),
                ))),
                msg_area,
            );
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

    // --- #785: optional display divisor for fractional render ---

    #[test]
    fn display_value_no_divisor_renders_as_integer() {
        let s = NumberStepper::new("count", 55, 0, 1000);
        assert_eq!(s.display_value(), "55");
    }

    #[test]
    fn display_value_divisor_10_value_55_renders_5_point_5() {
        let s = NumberStepper::new("per_session_usd", 55, 1, 1000).with_display_divisor(Some(10));
        assert_eq!(s.display_value(), "5.5");
    }

    #[test]
    fn display_value_divisor_10_value_0_renders_0_point_0() {
        let s = NumberStepper::new("budget", 0, 0, 1000).with_display_divisor(Some(10));
        assert_eq!(s.display_value(), "0.0");
    }

    #[test]
    fn display_value_divisor_10_value_125_renders_12_point_5() {
        let s = NumberStepper::new("total_usd", 125, 1, 10000).with_display_divisor(Some(10));
        assert_eq!(s.display_value(), "12.5");
    }

    #[test]
    fn display_value_divisor_one_renders_as_integer() {
        let s = NumberStepper::new("scale_one", 42, 0, 1000).with_display_divisor(Some(1));
        assert_eq!(s.display_value(), "42");
    }

    #[test]
    fn with_display_divisor_builder_preserves_other_fields() {
        let s = NumberStepper::new("x", 7, 1, 100)
            .with_step(3)
            .with_display_divisor(Some(10));
        assert_eq!(s.value, 7);
        assert_eq!(s.min, 1);
        assert_eq!(s.max, 100);
        assert_eq!(s.step, 3);
        assert_eq!(s.display_value(), "0.7");
    }
}
