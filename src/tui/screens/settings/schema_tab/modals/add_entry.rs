//! Add-entry modal: prompts for an identifier, validates it inline, and
//! returns a `ModalAction::Submit { id }` on success.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::tui::theme::Theme;

use super::super::widgets::identifier::{IdentifierError, validate_identifier};
use super::{ModalAction, centered_rect};

pub struct AddEntryModal {
    pub title: String,
    pub buffer: String,
    pub cursor: usize,
    pub last_error: Option<IdentifierError>,
    pub existing_ids: Vec<String>,
}

impl AddEntryModal {
    pub fn new(title: impl Into<String>, existing_ids: Vec<String>) -> Self {
        Self {
            title: title.into(),
            buffer: String::new(),
            cursor: 0,
            last_error: None,
            existing_ids,
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> ModalAction {
        match key.code {
            KeyCode::Esc => ModalAction::Cancel,
            KeyCode::Enter => {
                let existing_refs: Vec<&str> =
                    self.existing_ids.iter().map(|s| s.as_str()).collect();
                match validate_identifier(&self.buffer, &existing_refs) {
                    Ok(()) => ModalAction::Submit {
                        id: self.buffer.clone(),
                    },
                    Err(e) => {
                        self.last_error = Some(e);
                        ModalAction::None
                    }
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                    self.last_error = None;
                }
                ModalAction::None
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                ModalAction::None
            }
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    self.cursor += 1;
                }
                ModalAction::None
            }
            KeyCode::Char(c) => {
                if !c.is_ascii() {
                    return ModalAction::None;
                }
                self.buffer.insert(self.cursor, c);
                self.cursor += 1;
                self.last_error = None;
                ModalAction::None
            }
            _ => ModalAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let centered = centered_rect(area, 60, 9);
        f.render_widget(Clear, centered);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", self.title))
            .border_style(Style::default().fg(theme.border_active))
            .style(Style::default().bg(theme.branding_bg));
        f.render_widget(block.clone(), centered);

        let inner = block.inner(centered);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let prompt = Paragraph::new(Line::from(Span::styled(
            "Identifier (a-z, 0-9, -, _):",
            Style::default().fg(theme.text_primary),
        )));
        f.render_widget(prompt, chunks[0]);

        let cursor_char = if self.cursor < self.buffer.len() {
            &self.buffer[self.cursor..]
        } else {
            ""
        };
        let before = &self.buffer[..self.cursor];
        let after_cursor = cursor_char
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_else(|| " ".to_string());
        let after_rest = if cursor_char.len() > after_cursor.len() {
            &cursor_char[after_cursor.len()..]
        } else {
            ""
        };

        let input_line = Line::from(vec![
            Span::raw("> "),
            Span::raw(before),
            Span::styled(
                after_cursor,
                Style::default().add_modifier(Modifier::REVERSED),
            ),
            Span::raw(after_rest),
        ]);
        f.render_widget(Paragraph::new(input_line), chunks[1]);

        if let Some(err) = &self.last_error {
            let err_line = Line::from(Span::styled(
                err.message(),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ));
            f.render_widget(Paragraph::new(err_line), chunks[3]);
        }

        let footer = Paragraph::new(Line::from(Span::styled(
            "[Enter] confirm  [Esc] cancel",
            Style::default().fg(theme.text_muted),
        )))
        .alignment(Alignment::Center);
        f.render_widget(footer, chunks[4]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

    fn ev(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn typing_populates_buffer() {
        let mut m = AddEntryModal::new("Add agent", vec![]);
        assert_eq!(m.handle_input(ev(KeyCode::Char('a'))), ModalAction::None);
        assert_eq!(m.handle_input(ev(KeyCode::Char('b'))), ModalAction::None);
        assert_eq!(m.buffer, "ab");
    }

    #[test]
    fn enter_with_valid_id_returns_submit() {
        let mut m = AddEntryModal::new("Add agent", vec![]);
        m.handle_input(ev(KeyCode::Char('a')));
        let action = m.handle_input(ev(KeyCode::Enter));
        assert_eq!(action, ModalAction::Submit { id: "a".into() });
    }

    #[test]
    fn enter_with_invalid_format_sets_error_and_stays_open() {
        let mut m = AddEntryModal::new("Add agent", vec![]);
        for c in "AB".chars() {
            m.handle_input(ev(KeyCode::Char(c)));
        }
        let action = m.handle_input(ev(KeyCode::Enter));
        assert_eq!(action, ModalAction::None);
        assert!(matches!(m.last_error, Some(IdentifierError::InvalidFormat)));
    }

    #[test]
    fn enter_with_collision_sets_error() {
        let mut m = AddEntryModal::new("Add agent", vec!["gpt4".into()]);
        for c in "gpt4".chars() {
            m.handle_input(ev(KeyCode::Char(c)));
        }
        let action = m.handle_input(ev(KeyCode::Enter));
        assert_eq!(action, ModalAction::None);
        assert!(matches!(m.last_error, Some(IdentifierError::Collision(_))));
    }

    #[test]
    fn error_clears_on_next_char() {
        let mut m = AddEntryModal::new("Add agent", vec![]);
        m.handle_input(ev(KeyCode::Char('A')));
        m.handle_input(ev(KeyCode::Enter));
        assert!(m.last_error.is_some());
        m.handle_input(ev(KeyCode::Char('b')));
        assert!(m.last_error.is_none());
    }

    #[test]
    fn esc_returns_cancel() {
        let mut m = AddEntryModal::new("Add agent", vec![]);
        assert_eq!(m.handle_input(ev(KeyCode::Esc)), ModalAction::Cancel);
    }

    #[test]
    fn backspace_clears_error_too() {
        let mut m = AddEntryModal::new("Add agent", vec![]);
        m.handle_input(ev(KeyCode::Char('A')));
        m.handle_input(ev(KeyCode::Enter));
        assert!(m.last_error.is_some());
        m.handle_input(ev(KeyCode::Backspace));
        assert!(m.last_error.is_none());
    }
}
