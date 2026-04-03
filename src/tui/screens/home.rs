use super::ScreenAction;
use crate::tui::app::TuiMode;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const QUICK_ACTIONS: &[(&str, char)] = &[
    ("Browse Issues", 'i'),
    ("Browse Milestones", 'm'),
    ("Run Prompt", 'r'),
    ("Status", 's'),
    ("Cost Report", 'c'),
    ("Quit", 'q'),
];

const LOGO: &str = r#"
 ███╗   ███╗ █████╗ ███████╗███████╗████████╗██████╗  ██████╗
 ████╗ ████║██╔══██╗██╔════╝██╔════╝╚══██╔══╝██╔══██╗██╔═══██╗
 ██╔████╔██║███████║█████╗  ███████╗  ██║   ██████╔╝██║   ██║
 ██║╚██╔╝██║██╔══██║██╔══╝  ╚════██║  ██║   ██╔══██╗██║   ██║
 ██║ ╚═╝ ██║██║  ██║███████╗███████║  ██║   ██║  ██║╚██████╔╝
 ╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝  ╚═╝   ╚═╝  ╚═╝ ╚═════╝
"#;

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub repo: String,
    pub branch: String,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub issue_number: u64,
    pub title: String,
    pub status: String,
    pub cost_usd: f64,
}

pub struct HomeScreen {
    pub selected_action: usize,
    pub recent_sessions: Vec<SessionSummary>,
    pub project_info: ProjectInfo,
    pub warnings: Vec<String>,
}

impl HomeScreen {
    pub const NUM_ACTIONS: usize = QUICK_ACTIONS.len();
    pub const QUIT_ACTION_INDEX: usize = 5;

    pub fn new(
        project_info: ProjectInfo,
        recent_sessions: Vec<SessionSummary>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            selected_action: 0,
            recent_sessions,
            project_info,
            warnings,
        }
    }

    pub fn handle_input(&mut self, event: &Event) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                // Direct shortcut keys
                KeyCode::Char('i') => return ScreenAction::Push(TuiMode::IssueBrowser),
                KeyCode::Char('m') => return ScreenAction::Push(TuiMode::MilestoneView),
                KeyCode::Char('s') => return ScreenAction::Push(TuiMode::Overview),
                KeyCode::Char('c') => return ScreenAction::Push(TuiMode::CostDashboard),
                KeyCode::Char('q') => return ScreenAction::Quit,
                // Navigation
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.selected_action < Self::NUM_ACTIONS - 1 {
                        self.selected_action += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected_action = self.selected_action.saturating_sub(1);
                }
                // Enter executes the selected action
                KeyCode::Enter => {
                    return self.execute_selected_action();
                }
                KeyCode::Esc => return ScreenAction::None,
                _ => {}
            }
        }
        ScreenAction::None
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let warning_height = if self.warnings.is_empty() {
            0
        } else {
            (self.warnings.len() as u16 + 2).min(6)
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),              // logo
                Constraint::Length(3),              // project info
                Constraint::Length(warning_height), // warnings (0 if none)
                Constraint::Min(8),                 // quick actions + recent sessions
            ])
            .split(area);

        self.draw_logo(f, chunks[0]);
        self.draw_project_info(f, chunks[1]);

        if !self.warnings.is_empty() {
            self.draw_warnings(f, chunks[2]);
        }

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[3]);

        self.draw_quick_actions(f, bottom[0]);
        self.draw_recent_sessions(f, bottom[1]);
    }

    pub fn tick(&mut self) {
        // No-op for now; could refresh recent sessions
    }

    fn execute_selected_action(&self) -> ScreenAction {
        match self.selected_action {
            0 => ScreenAction::Push(TuiMode::IssueBrowser),
            1 => ScreenAction::Push(TuiMode::MilestoneView),
            2 => ScreenAction::Push(TuiMode::Overview), // Run prompt placeholder
            3 => ScreenAction::Push(TuiMode::Overview),
            4 => ScreenAction::Push(TuiMode::CostDashboard),
            5 => ScreenAction::Quit,
            _ => ScreenAction::None,
        }
    }

    fn draw_logo(&self, f: &mut Frame, area: Rect) {
        let logo = Paragraph::new(LOGO)
            .style(Style::default().fg(Color::Green))
            .alignment(Alignment::Center);
        f.render_widget(logo, area);
    }

    fn draw_project_info(&self, f: &mut Frame, area: Rect) {
        let info = Line::from(vec![
            Span::styled("  Repo: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&self.project_info.repo, Style::default().fg(Color::Cyan)),
            Span::raw("  |  "),
            Span::styled("Branch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &self.project_info.branch,
                Style::default().fg(Color::Yellow),
            ),
        ]);
        let block = Block::default().borders(Borders::BOTTOM);
        let para = Paragraph::new(info)
            .block(block)
            .alignment(Alignment::Center);
        f.render_widget(para, area);
    }

    fn draw_warnings(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Warnings ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let lines: Vec<Line> = self
            .warnings
            .iter()
            .map(|w| {
                Line::from(vec![
                    Span::styled("  ! ", Style::default().fg(Color::Yellow)),
                    Span::styled(w.as_str(), Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_quick_actions(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Quick Actions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let selected_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);

        let mut lines = Vec::new();
        for (idx, (label, key)) in QUICK_ACTIONS.iter().enumerate() {
            let is_selected = idx == self.selected_action;
            let style = if is_selected {
                selected_style
            } else {
                Style::default().fg(Color::White)
            };
            let key_style = if is_selected {
                selected_style
            } else {
                Style::default().fg(Color::Green)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  [{}]  ", key), key_style),
                Span::styled(*label, style),
            ]));
        }

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_recent_sessions(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Recent Activity ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        if self.recent_sessions.is_empty() {
            let para = Paragraph::new("  No recent sessions")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        let lines: Vec<Line> = self
            .recent_sessions
            .iter()
            .map(|s| {
                let status_style = match s.status.as_str() {
                    "completed" => Style::default().fg(Color::Green),
                    "running" => Style::default().fg(Color::Yellow),
                    "errored" => Style::default().fg(Color::Red),
                    _ => Style::default().fg(Color::DarkGray),
                };
                let symbol = match s.status.as_str() {
                    "completed" => "✅",
                    "running" => "▶ ",
                    "errored" => "❌",
                    _ => "⏳",
                };
                Line::from(vec![
                    Span::styled(format!("  {} ", symbol), status_style),
                    Span::styled(
                        format!("#{}", s.issue_number),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw(" "),
                    Span::styled(&s.title, Style::default().fg(Color::White)),
                    Span::styled(
                        format!("  ${:.2}", s.cost_usd),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;
    use crossterm::event::KeyCode;

    fn make_project_info() -> ProjectInfo {
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
        }
    }

    fn make_session_summary(id: u64) -> SessionSummary {
        SessionSummary {
            issue_number: id,
            title: format!("Issue #{}", id),
            status: "completed".to_string(),
            cost_usd: 0.05,
        }
    }

    #[test]
    fn home_initial_selected_action_is_zero() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert_eq!(screen.selected_action, 0);
    }

    #[test]
    fn home_key_j_moves_selection_down() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(action, ScreenAction::None);
        assert_eq!(screen.selected_action, 1);
    }

    #[test]
    fn home_key_down_moves_selection_down() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Down));
        assert_eq!(screen.selected_action, 1);
    }

    #[test]
    fn home_key_k_moves_selection_up() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_action, 2);
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected_action, 1);
    }

    #[test]
    fn home_key_k_does_not_underflow_at_zero() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Char('k')));
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected_action, 0);
    }

    #[test]
    fn home_key_j_does_not_overflow_past_last_action() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let num_actions = HomeScreen::NUM_ACTIONS;
        for _ in 0..num_actions + 5 {
            screen.handle_input(&key_event(KeyCode::Char('j')));
        }
        assert_eq!(screen.selected_action, num_actions - 1);
    }

    #[test]
    fn home_key_up_moves_selection_up() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Down));
        screen.handle_input(&key_event(KeyCode::Up));
        assert_eq!(screen.selected_action, 0);
    }

    #[test]
    fn home_key_i_returns_push_issue_browser() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('i')));
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_key_m_returns_push_milestone_view() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('m')));
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_key_q_returns_quit() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('q')));
        assert_eq!(action, ScreenAction::Quit);
    }

    #[test]
    fn home_enter_on_issues_action_returns_push_issue_browser() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_enter_on_milestones_action_returns_push_milestone_view() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Down));
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_enter_on_quit_action_returns_quit() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        for _ in 0..HomeScreen::QUIT_ACTION_INDEX {
            screen.handle_input(&key_event(KeyCode::Down));
        }
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::Quit);
    }

    #[test]
    fn home_esc_returns_none() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Esc));
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn home_tick_does_not_panic() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.tick();
        screen.tick();
        screen.tick();
    }

    #[test]
    fn home_recent_sessions_stored() {
        let sessions = vec![make_session_summary(10), make_session_summary(11)];
        let screen = HomeScreen::new(make_project_info(), sessions, vec![]);
        assert_eq!(screen.recent_sessions.len(), 2);
        assert_eq!(screen.recent_sessions[0].issue_number, 10);
        assert_eq!(screen.recent_sessions[1].issue_number, 11);
    }

    #[test]
    fn home_unknown_key_returns_none() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('x')));
        assert_eq!(action, ScreenAction::None);
    }
}
