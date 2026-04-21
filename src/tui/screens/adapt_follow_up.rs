//! Overlay screen shown after an adapt session completes, presenting the
//! parsed "next iteration paths" as a selectable list.
//!
//! Matches the pattern established by `HollowRetryScreen`: centered popup,
//! j/k or number keys to select, Enter to confirm, Esc to dismiss.

use super::{PromptSessionConfig, Screen, ScreenAction, draw_keybinds_bar};
use crate::adapt::suggestions::build_follow_up_prompt;
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

/// Screen shown when an adapt session completes with parsed suggestions.
pub struct AdaptFollowUpScreen {
    pub session_label: String,
    pub suggestions: Vec<String>,
    pub selected: usize,
}

impl AdaptFollowUpScreen {
    pub fn new(session_label: String, suggestions: Vec<String>) -> Self {
        Self {
            session_label,
            suggestions,
            selected: 0,
        }
    }

    fn move_down(&mut self) {
        if !self.suggestions.is_empty() && self.selected + 1 < self.suggestions.len() {
            self.selected += 1;
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn current_prompt(&self) -> Option<PromptSessionConfig> {
        let direction = self.suggestions.get(self.selected)?;
        Some(PromptSessionConfig {
            prompt: build_follow_up_prompt(direction),
            image_paths: Vec::new(),
        })
    }
}

impl KeymapProvider for AdaptFollowUpScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Adapt Follow-up",
            bindings: vec![
                KeyBinding {
                    key: "j/k",
                    description: "Move selection",
                },
                KeyBinding {
                    key: "1-9",
                    description: "Select by index",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Execute direction",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Dismiss",
                },
            ],
        }]
    }
}

impl Screen for AdaptFollowUpScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.move_down();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.move_up();
                }
                KeyCode::Char(c) if c.is_ascii_digit() && *c != '0' => {
                    let idx = (*c as usize) - ('1' as usize);
                    if idx < self.suggestions.len() {
                        self.selected = idx;
                        if let Some(cfg) = self.current_prompt() {
                            return ScreenAction::LaunchPromptSession(cfg);
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(cfg) = self.current_prompt() {
                        return ScreenAction::LaunchPromptSession(cfg);
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return ScreenAction::Pop;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let items = self.suggestions.len() as u16;
        let popup_width = 80.min(area.width.saturating_sub(4));
        let popup_height = (7 + items).min(area.height.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        f.render_widget(Clear, popup_area);

        let block = theme
            .styled_block("Next iteration paths", false)
            .border_style(
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            );

        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // header
                Constraint::Length(1),  // blank
                Constraint::Min(items), // list
                Constraint::Length(1),  // blank
                Constraint::Length(1),  // keybinds
            ])
            .split(inner);

        let header = Paragraph::new(Line::from(vec![
            Span::styled("Session: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                &self.session_label,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        f.render_widget(header, chunks[0]);

        let list_lines: Vec<Line> = self
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let is_selected = i == self.selected;
                let marker_style = if is_selected {
                    Style::default()
                        .fg(theme.branding_fg)
                        .bg(theme.accent_info)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.accent_info)
                };
                let text_style = if is_selected {
                    Style::default()
                        .fg(theme.branding_fg)
                        .bg(theme.accent_info)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                Line::from(vec![
                    Span::styled(format!(" {}. ", i + 1), marker_style),
                    Span::styled(s.clone(), text_style),
                ])
            })
            .collect();

        let list = Paragraph::new(list_lines).wrap(Wrap { trim: false });
        f.render_widget(list, chunks[2]);

        draw_keybinds_bar(
            f,
            chunks[4],
            &[
                ("j/k", "Move"),
                ("1-9", "Pick"),
                ("Enter", "Execute"),
                ("Esc", "Dismiss"),
            ],
            theme,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;

    fn make_screen() -> AdaptFollowUpScreen {
        AdaptFollowUpScreen::new(
            "adapt".into(),
            vec![
                "Burn down M0".into(),
                "Fill docs".into(),
                "Observability".into(),
            ],
        )
    }

    #[test]
    fn j_moves_selection_down() {
        let mut s = make_screen();
        s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn k_moves_selection_up() {
        let mut s = make_screen();
        s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn k_does_not_underflow() {
        let mut s = make_screen();
        s.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn j_does_not_overflow() {
        let mut s = make_screen();
        for _ in 0..10 {
            s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn enter_launches_prompt_session() {
        let mut s = make_screen();
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchPromptSession(cfg) => {
                assert!(cfg.prompt.contains("Burn down M0"));
            }
            other => panic!("expected LaunchPromptSession, got {:?}", other),
        }
    }

    #[test]
    fn digit_key_selects_and_launches() {
        let mut s = make_screen();
        let action = s.handle_input(&key_event(KeyCode::Char('2')), InputMode::Normal);
        match action {
            ScreenAction::LaunchPromptSession(cfg) => {
                assert_eq!(s.selected, 1);
                assert!(cfg.prompt.contains("Fill docs"));
            }
            other => panic!("expected LaunchPromptSession, got {:?}", other),
        }
    }

    #[test]
    fn esc_dismisses() {
        let mut s = make_screen();
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn q_dismisses() {
        let mut s = make_screen();
        let action = s.handle_input(&key_event(KeyCode::Char('q')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn empty_suggestions_enter_returns_none() {
        let mut s = AdaptFollowUpScreen::new("adapt".into(), vec![]);
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn digit_beyond_list_is_noop() {
        let mut s = make_screen();
        let action = s.handle_input(&key_event(KeyCode::Char('9')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn initial_selected_is_zero() {
        let s = make_screen();
        assert_eq!(s.selected, 0);
    }
}
