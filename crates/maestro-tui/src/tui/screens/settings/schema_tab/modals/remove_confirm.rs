//! Remove-confirm modal: echoes the section id and waits for `[y]`/`Enter`
//! to confirm, `[n]`/`Esc` to cancel.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::theme::Theme;

use super::{ModalAction, centered_rect};

pub struct RemoveConfirmModal {
    pub section_label: String,
}

impl RemoveConfirmModal {
    pub fn new(section_label: impl Into<String>) -> Self {
        Self {
            section_label: section_label.into(),
        }
    }

    pub fn handle_input(&self, key: KeyEvent) -> ModalAction {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => ModalAction::Submit {
                id: self.section_label.clone(),
            },
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => ModalAction::Cancel,
            _ => ModalAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let centered = centered_rect(area, 60, 7);
        f.render_widget(Clear, centered);

        let block = theme
            .styled_block("Remove entry", true)
            .border_style(Style::default().fg(theme.accent_error));
        let inner = block.inner(centered);
        f.render_widget(block, centered);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let prompt = Paragraph::new(Line::from(vec![
            Span::raw("Delete "),
            Span::styled(
                format!("[{}]", self.section_label),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("?"),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(prompt, chunks[1]);

        let footer = Paragraph::new(Line::from(Span::styled(
            "[y] confirm  [n/Esc] cancel",
            Style::default().fg(theme.text_muted),
        )))
        .alignment(Alignment::Center);
        f.render_widget(footer, chunks[3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn ev(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn y_confirms() {
        let m = RemoveConfirmModal::new("agents.claude");
        let action = m.handle_input(ev(KeyCode::Char('y')));
        assert_eq!(
            action,
            ModalAction::Submit {
                id: "agents.claude".into()
            }
        );
    }

    #[test]
    fn enter_confirms() {
        let m = RemoveConfirmModal::new("agents.claude");
        let action = m.handle_input(ev(KeyCode::Enter));
        assert!(matches!(action, ModalAction::Submit { .. }));
    }

    #[test]
    fn n_cancels() {
        let m = RemoveConfirmModal::new("agents.claude");
        let action = m.handle_input(ev(KeyCode::Char('n')));
        assert_eq!(action, ModalAction::Cancel);
    }

    #[test]
    fn esc_cancels() {
        let m = RemoveConfirmModal::new("agents.claude");
        let action = m.handle_input(ev(KeyCode::Esc));
        assert_eq!(action, ModalAction::Cancel);
    }

    #[test]
    fn other_keys_are_noop() {
        let m = RemoveConfirmModal::new("agents.claude");
        assert_eq!(m.handle_input(ev(KeyCode::Char('x'))), ModalAction::None);
        assert_eq!(m.handle_input(ev(KeyCode::Tab)), ModalAction::None);
    }
}
