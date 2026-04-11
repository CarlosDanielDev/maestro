use super::{Screen, ScreenAction, draw_keybinds_bar};
use crate::tui::app::TuiMode;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};
use uuid::Uuid;

/// Screen shown when a hollow completion exceeds auto-retry limits.
pub struct HollowRetryScreen {
    pub session_id: Uuid,
    pub session_label: String,
    pub retry_count: u32,
    pub max_retries: u32,
}

impl HollowRetryScreen {
    pub fn new(
        session_id: Uuid,
        session_label: String,
        retry_count: u32,
        max_retries: u32,
    ) -> Self {
        Self {
            session_id,
            session_label,
            retry_count,
            max_retries,
        }
    }
}

impl KeymapProvider for HollowRetryScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Hollow Retry",
            bindings: vec![
                KeyBinding {
                    key: "r",
                    description: "Retry session",
                },
                KeyBinding {
                    key: "s/Esc",
                    description: "Skip",
                },
                KeyBinding {
                    key: "v",
                    description: "View logs",
                },
            ],
        }]
    }
}

impl Screen for HollowRetryScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Char('r') => {
                    return ScreenAction::RetryHollow(self.session_id);
                }
                KeyCode::Char('s') | KeyCode::Esc => {
                    return ScreenAction::Pop;
                }
                KeyCode::Char('v') => {
                    return ScreenAction::Push(TuiMode::Detail(self.session_id));
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Center a box in the area
        let popup_width = 50.min(area.width.saturating_sub(4));
        let popup_height = 10.min(area.height.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        f.render_widget(Clear, popup_area);

        let block = theme
            .styled_block("Hollow Completion Detected", false)
            .border_style(
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            );

        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // info
                Constraint::Length(3), // message
                Constraint::Min(1),    // keybinds
            ])
            .split(inner);

        let info = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Session: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    &self.session_label,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Retries: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    format!("{}/{}", self.retry_count, self.max_retries),
                    Style::default().fg(theme.accent_error),
                ),
            ]),
        ]);
        f.render_widget(info, chunks[0]);

        let message =
            Paragraph::new("The session completed without performing any observable work.")
                .style(Style::default().fg(theme.accent_warning))
                .wrap(Wrap { trim: true });
        f.render_widget(message, chunks[1]);

        draw_keybinds_bar(
            f,
            chunks[2],
            &[("r", "Retry"), ("s", "Skip"), ("v", "View Logs")],
            theme,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;

    #[test]
    fn r_key_returns_retry_hollow_action() {
        let id = Uuid::new_v4();
        let mut screen = HollowRetryScreen::new(id, "S-test".into(), 1, 1);
        let action = screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert_eq!(action, ScreenAction::RetryHollow(id));
    }

    #[test]
    fn s_key_returns_pop() {
        let id = Uuid::new_v4();
        let mut screen = HollowRetryScreen::new(id, "S-test".into(), 1, 1);
        let action = screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn esc_returns_pop() {
        let id = Uuid::new_v4();
        let mut screen = HollowRetryScreen::new(id, "S-test".into(), 1, 1);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn v_key_returns_push_detail() {
        let id = Uuid::new_v4();
        let mut screen = HollowRetryScreen::new(id, "S-test".into(), 1, 1);
        let action = screen.handle_input(&key_event(KeyCode::Char('v')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::Detail(id)));
    }
}
