//! CI Error Review screen (#695).
//!
//! Popup over the Overview that lets the user inspect failed-check logs
//! and the planned local gate command before launching a fix session.

use super::{CiFixConfig, Screen, ScreenAction, draw_keybinds_bar};
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

/// Lifecycle of the log fetch behind the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchPhase {
    /// Fetch is in flight; the screen shows a loading hint.
    Loading,
    /// Logs are available; the user may confirm to launch the fix session.
    Ready { log_excerpt: String },
    /// Fetch failed; the user can still proceed (the agent gets the
    /// failed-check names without log body).
    Failed { reason: String },
}

/// State backing the CI Error Review popup.
#[derive(Debug, Clone)]
pub struct CiErrorReviewState {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub failed_check_names: Vec<String>,
    pub planned_gate_cmd: Option<String>,
    pub fetch: FetchPhase,
}

/// The popup screen.
pub struct CiErrorReviewScreen {
    pub state: CiErrorReviewState,
    /// Vertical scroll offset for the log excerpt body.
    pub scroll_offset: u16,
}

impl CiErrorReviewScreen {
    pub fn new(state: CiErrorReviewState) -> Self {
        Self {
            state,
            scroll_offset: 0,
        }
    }

    fn build_launch_action(&self) -> ScreenAction {
        let failure_log = match &self.state.fetch {
            FetchPhase::Ready { log_excerpt } => log_excerpt.clone(),
            FetchPhase::Failed { reason } => format!("(log fetch failed: {})", reason),
            // The `Enter` arm in `handle_input` already short-circuits on
            // `Loading`; reaching here is a regression in that guard.
            FetchPhase::Loading => {
                unreachable!("build_launch_action must not be called while Loading")
            }
        };
        ScreenAction::LaunchCiFix(CiFixConfig {
            pr_number: self.state.pr_number,
            issue_number: self.state.issue_number,
            branch: self.state.branch.clone(),
            local_gate_cmd: self.state.planned_gate_cmd.clone(),
            failure_log,
            // `attempt` is overridden by `launch_ci_fix_from_review` from the
            // matching `PendingPrCheck.fix_attempt + 1`; this default is a
            // placeholder.
            attempt: 1,
        })
    }
}

impl KeymapProvider for CiErrorReviewScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "CI Error Review",
            bindings: vec![
                KeyBinding {
                    key: "Enter",
                    description: "Launch fix session",
                },
                KeyBinding {
                    key: "Esc/q",
                    description: "Abort",
                },
                KeyBinding {
                    key: "j/k",
                    description: "Scroll log",
                },
            ],
        }]
    }
}

impl Screen for CiErrorReviewScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Enter => {
                    if matches!(self.state.fetch, FetchPhase::Loading) {
                        return ScreenAction::None;
                    }
                    return self.build_launch_action();
                }
                KeyCode::Esc | KeyCode::Char('q') => return ScreenAction::Pop,
                KeyCode::Char('j') | KeyCode::Down => {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let popup_width = 80.min(area.width.saturating_sub(4));
        let popup_height = 20.min(area.height.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        f.render_widget(Clear, popup_area);

        let block = theme.styled_block("CI Error Review", false).border_style(
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        );
        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Length(2), // gate cmd
                Constraint::Min(3),    // log body
                Constraint::Length(1), // keybinds
            ])
            .split(inner);

        let header = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("PR #", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    self.state.pr_number.to_string(),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled("Issue #", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    self.state.issue_number.to_string(),
                    Style::default().fg(theme.text_primary),
                ),
            ]),
            Line::from(vec![
                Span::styled("Branch: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    self.state.branch.clone(),
                    Style::default().fg(theme.text_primary),
                ),
            ]),
            Line::from(vec![
                Span::styled("Failed: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    self.state.failed_check_names.join(", "),
                    Style::default().fg(theme.accent_error),
                ),
            ]),
        ]);
        f.render_widget(header, chunks[0]);

        let gate_line = match &self.state.planned_gate_cmd {
            Some(cmd) => Line::from(vec![
                Span::styled("Will run: ", Style::default().fg(theme.text_secondary)),
                Span::styled(cmd.clone(), Style::default().fg(theme.accent_success)),
            ]),
            None => Line::from(vec![Span::styled(
                "No matching local gate detected — agent will use its own judgment.",
                Style::default().fg(theme.text_muted),
            )]),
        };
        f.render_widget(Paragraph::new(gate_line), chunks[1]);

        let body = match &self.state.fetch {
            FetchPhase::Loading => Paragraph::new(Line::from(Span::styled(
                "Fetching failed check logs…",
                Style::default().fg(theme.text_muted),
            ))),
            FetchPhase::Ready { log_excerpt } => Paragraph::new(log_excerpt.as_str())
                .style(Style::default().fg(theme.text_primary))
                .wrap(Wrap { trim: false })
                .scroll((self.scroll_offset, 0)),
            FetchPhase::Failed { reason } => Paragraph::new(format!(
                "Log fetch failed: {}\n\n(You can still press Enter to launch the fix session — the agent will get the failed check names without the log body.)",
                reason
            ))
            .style(Style::default().fg(theme.accent_warning))
            .wrap(Wrap { trim: true }),
        };
        f.render_widget(body, chunks[2]);

        draw_keybinds_bar(
            f,
            chunks[3],
            &[("Enter", "Launch fix"), ("Esc", "Abort"), ("j/k", "Scroll")],
            theme,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;

    fn ready_state() -> CiErrorReviewState {
        CiErrorReviewState {
            pr_number: 42,
            issue_number: 10,
            branch: "feat/x".into(),
            failed_check_names: vec!["clippy".into()],
            planned_gate_cmd: Some("cargo clippy".into()),
            fetch: FetchPhase::Ready {
                log_excerpt: "log".into(),
            },
        }
    }

    #[test]
    fn enter_from_ready_returns_launch_ci_fix_with_full_config() {
        let mut screen = CiErrorReviewScreen::new(ready_state());
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchCiFix(cfg) => {
                assert_eq!(cfg.pr_number, 42);
                assert_eq!(cfg.issue_number, 10);
                assert_eq!(cfg.branch, "feat/x");
                assert_eq!(cfg.local_gate_cmd.as_deref(), Some("cargo clippy"));
                assert_eq!(cfg.failure_log, "log");
            }
            other => panic!("expected LaunchCiFix, got {:?}", other),
        }
    }

    #[test]
    fn enter_from_loading_is_noop() {
        let mut state = ready_state();
        state.fetch = FetchPhase::Loading;
        let mut screen = CiErrorReviewScreen::new(state);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn enter_from_failed_returns_launch_ci_fix_with_reason_in_log() {
        let mut state = ready_state();
        state.fetch = FetchPhase::Failed {
            reason: "network error".into(),
        };
        let mut screen = CiErrorReviewScreen::new(state);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchCiFix(cfg) => {
                assert!(
                    cfg.failure_log.contains("network error"),
                    "failure log should embed the fetch error reason; got: {}",
                    cfg.failure_log
                );
            }
            other => panic!("expected LaunchCiFix, got {:?}", other),
        }
    }

    #[test]
    fn esc_returns_pop() {
        let mut screen = CiErrorReviewScreen::new(ready_state());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn q_returns_pop() {
        let mut screen = CiErrorReviewScreen::new(ready_state());
        let action = screen.handle_input(&key_event(KeyCode::Char('q')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn j_scrolls_down() {
        let mut screen = CiErrorReviewScreen::new(ready_state());
        let _ = screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 1);
    }

    #[test]
    fn k_scrolls_up_clamped_at_zero() {
        let mut screen = CiErrorReviewScreen::new(ready_state());
        assert_eq!(screen.scroll_offset, 0);
        let _ = screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 0, "k must not underflow below zero");
    }
}
