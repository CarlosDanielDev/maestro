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
    pub username: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub issue_number: u64,
    pub title: String,
    pub status: String,
    pub cost_usd: f64,
}

/// The kind of suggestion determines its icon, color, and action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionKind {
    /// N issues are labeled maestro:ready
    ReadyIssues { count: usize },
    /// Milestone progress report
    MilestoneProgress {
        title: String,
        closed: u32,
        total: u32,
    },
    /// No sessions are currently running
    IdleSessions,
    /// Issues labeled maestro:failed exist
    FailedIssues { count: usize },
}

/// A single actionable suggestion displayed on the home screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub kind: SuggestionKind,
    pub message: String,
    pub action: TuiMode,
}

impl Suggestion {
    /// Build contextual suggestions from GitHub data and session state.
    pub fn build_suggestions(
        ready_issue_count: usize,
        failed_issue_count: usize,
        milestones: &[(String, u32, u32)],
        active_session_count: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if ready_issue_count > 0 {
            suggestions.push(Suggestion {
                kind: SuggestionKind::ReadyIssues {
                    count: ready_issue_count,
                },
                message: format!(
                    "{} issue{} labeled maestro:ready — press [i] to browse",
                    ready_issue_count,
                    if ready_issue_count == 1 { "" } else { "s" }
                ),

                action: TuiMode::IssueBrowser,
            });
        }

        for (title, closed, total) in milestones {
            if *total > 0 {
                let pct = (*closed as f64 / *total as f64 * 100.0).clamp(0.0, 100.0) as u32;
                suggestions.push(Suggestion {
                    kind: SuggestionKind::MilestoneProgress {
                        title: title.clone(),
                        closed: *closed,
                        total: *total,
                    },
                    message: format!(
                        "Milestone {} is {}% complete ({}/{} closed)",
                        title, pct, closed, total
                    ),

                    action: TuiMode::MilestoneView,
                });
            }
        }

        if failed_issue_count > 0 {
            suggestions.push(Suggestion {
                kind: SuggestionKind::FailedIssues {
                    count: failed_issue_count,
                },
                message: format!(
                    "{} issue{} labeled maestro:failed — press [i] to review",
                    failed_issue_count,
                    if failed_issue_count == 1 { "" } else { "s" }
                ),

                action: TuiMode::IssueBrowser,
            });
        }

        if active_session_count == 0 {
            suggestions.push(Suggestion {
                kind: SuggestionKind::IdleSessions,
                message: "No sessions running — press [r] to start".to_string(),

                action: TuiMode::Overview,
            });
        }

        suggestions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeSection {
    QuickActions,
    Suggestions,
}

pub struct HomeScreen {
    pub selected_action: usize,
    pub recent_sessions: Vec<SessionSummary>,
    pub project_info: ProjectInfo,
    pub warnings: Vec<String>,
    pub suggestions: Vec<Suggestion>,
    pub selected_suggestion: usize,
    pub focus_section: HomeSection,
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
            suggestions: Vec::new(),
            selected_suggestion: 0,
            focus_section: HomeSection::QuickActions,
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
                KeyCode::Char('i') => return ScreenAction::Push(TuiMode::IssueBrowser),
                KeyCode::Char('m') => return ScreenAction::Push(TuiMode::MilestoneView),
                KeyCode::Char('s') => return ScreenAction::Push(TuiMode::Overview),
                KeyCode::Char('c') => return ScreenAction::Push(TuiMode::CostDashboard),
                KeyCode::Char('q') => return ScreenAction::Quit,
                KeyCode::Tab => {
                    self.focus_section = match self.focus_section {
                        HomeSection::QuickActions => HomeSection::Suggestions,
                        HomeSection::Suggestions => HomeSection::QuickActions,
                    };
                }
                KeyCode::Char('j') | KeyCode::Down => match self.focus_section {
                    HomeSection::QuickActions => {
                        if self.selected_action < Self::NUM_ACTIONS - 1 {
                            self.selected_action += 1;
                        }
                    }
                    HomeSection::Suggestions => {
                        if !self.suggestions.is_empty()
                            && self.selected_suggestion < self.suggestions.len() - 1
                        {
                            self.selected_suggestion += 1;
                        }
                    }
                },
                KeyCode::Char('k') | KeyCode::Up => match self.focus_section {
                    HomeSection::QuickActions => {
                        self.selected_action = self.selected_action.saturating_sub(1);
                    }
                    HomeSection::Suggestions => {
                        self.selected_suggestion = self.selected_suggestion.saturating_sub(1);
                    }
                },
                KeyCode::Enter => match self.focus_section {
                    HomeSection::QuickActions => {
                        return self.execute_selected_action();
                    }
                    HomeSection::Suggestions => {
                        if let Some(suggestion) = self.suggestions.get(self.selected_suggestion) {
                            return ScreenAction::Push(suggestion.action);
                        }
                        return ScreenAction::None;
                    }
                },
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
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(35),
                Constraint::Percentage(35),
            ])
            .split(chunks[3]);

        self.draw_quick_actions(f, bottom[0]);
        self.draw_suggestions(f, bottom[1]);
        self.draw_recent_sessions(f, bottom[2]);
    }

    pub fn set_suggestions(&mut self, suggestions: Vec<Suggestion>) {
        self.suggestions = suggestions;
        self.selected_suggestion = 0;
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
        let username_display = self.project_info.username.as_deref().unwrap_or("unknown");

        let info = Line::from(vec![
            Span::styled("  Repo: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&self.project_info.repo, Style::default().fg(Color::Cyan)),
            Span::raw("  |  "),
            Span::styled("Branch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &self.project_info.branch,
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  |  "),
            Span::styled("User: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("@{}", username_display),
                Style::default().fg(Color::Green),
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
        let is_focused = self.focus_section == HomeSection::QuickActions;
        let border_color = if is_focused {
            Color::Green
        } else {
            Color::DarkGray
        };
        let block = Block::default()
            .title(" Quick Actions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let selected_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);

        let mut lines = Vec::new();
        for (idx, (label, key)) in QUICK_ACTIONS.iter().enumerate() {
            let is_selected = is_focused && idx == self.selected_action;
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

    fn draw_suggestions(&self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus_section == HomeSection::Suggestions;
        let border_color = if is_focused {
            Color::Green
        } else {
            Color::DarkGray
        };
        let block = Block::default()
            .title(" Suggestions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.suggestions.is_empty() {
            let para = Paragraph::new("  No suggestions — everything looks good!")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        let mut lines = Vec::new();
        for (idx, suggestion) in self.suggestions.iter().enumerate() {
            let is_selected = is_focused && idx == self.selected_suggestion;
            let icon = match &suggestion.kind {
                SuggestionKind::ReadyIssues { .. } => ">>",
                SuggestionKind::MilestoneProgress { .. } => "~~",
                SuggestionKind::IdleSessions => "--",
                SuggestionKind::FailedIssues { .. } => "!!",
            };
            let color = match &suggestion.kind {
                SuggestionKind::ReadyIssues { .. } => Color::Green,
                SuggestionKind::MilestoneProgress { .. } => Color::Cyan,
                SuggestionKind::IdleSessions => Color::Yellow,
                SuggestionKind::FailedIssues { .. } => Color::Red,
            };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), Style::default().fg(color)),
                Span::styled(&suggestion.message, style),
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
            username: None,
        }
    }

    fn make_project_info_with_user(name: &str) -> ProjectInfo {
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some(name.to_string()),
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

    // --- Tests for ProjectInfo.username field (Issue #34) ---

    #[test]
    fn project_info_with_user_stores_username() {
        let info = make_project_info_with_user("carlos");
        assert_eq!(info.username, Some("carlos".to_string()));
    }

    #[test]
    fn project_info_without_user_is_none() {
        let info = make_project_info();
        assert!(info.username.is_none());
    }

    #[test]
    fn home_screen_stores_project_info_with_user() {
        let info = make_project_info_with_user("testuser");
        let screen = HomeScreen::new(info, vec![], vec![]);
        assert_eq!(screen.project_info.username, Some("testuser".to_string()));
    }

    #[test]
    fn home_screen_stores_project_info_without_user() {
        let info = make_project_info();
        let screen = HomeScreen::new(info, vec![], vec![]);
        assert!(screen.project_info.username.is_none());
    }

    // --- Tests for Work Suggestions (Issue #35) ---

    fn make_home_with_suggestions(suggestions: Vec<Suggestion>) -> HomeScreen {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.suggestions = suggestions;
        screen
    }

    fn focus_suggestions(screen: &mut HomeScreen) {
        screen.handle_input(&key_event(KeyCode::Tab));
    }

    // -- Suggestion::build_suggestions (pure logic) --

    #[test]
    fn build_suggestions_with_ready_issues_emits_ready_issues_suggestion() {
        let result = Suggestion::build_suggestions(3, 0, &[], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, SuggestionKind::ReadyIssues { count: 3 });

        assert_eq!(result[0].action, TuiMode::IssueBrowser);
    }

    #[test]
    fn build_suggestions_with_zero_ready_issues_emits_no_ready_suggestion() {
        let result = Suggestion::build_suggestions(0, 0, &[], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn build_suggestions_with_failed_issues_emits_failed_issues_suggestion() {
        let result = Suggestion::build_suggestions(0, 2, &[], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, SuggestionKind::FailedIssues { count: 2 });

        assert_eq!(result[0].action, TuiMode::IssueBrowser);
    }

    #[test]
    fn build_suggestions_with_milestone_emits_milestone_progress_suggestion() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1.0".to_string(), 3, 10)], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].kind,
            SuggestionKind::MilestoneProgress {
                title: "v1.0".to_string(),
                closed: 3,
                total: 10,
            }
        );

        assert_eq!(result[0].action, TuiMode::MilestoneView);
    }

    #[test]
    fn build_suggestions_milestone_with_zero_total_is_skipped() {
        let result = Suggestion::build_suggestions(0, 0, &[("empty".to_string(), 0, 0)], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn build_suggestions_multiple_milestones_emits_one_per_nonzero() {
        let milestones = vec![
            ("v1".to_string(), 1u32, 5u32),
            ("v2".to_string(), 0u32, 0u32),
        ];
        let result = Suggestion::build_suggestions(0, 0, &milestones, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].kind,
            SuggestionKind::MilestoneProgress {
                title: "v1".to_string(),
                closed: 1,
                total: 5,
            }
        );
    }

    #[test]
    fn build_suggestions_with_no_active_sessions_emits_idle_sessions_suggestion() {
        let result = Suggestion::build_suggestions(0, 0, &[], 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, SuggestionKind::IdleSessions);

        assert_eq!(result[0].action, TuiMode::Overview);
    }

    #[test]
    fn build_suggestions_with_active_sessions_does_not_emit_idle() {
        let result = Suggestion::build_suggestions(0, 0, &[], 2);
        assert!(
            result
                .iter()
                .all(|s| s.kind != SuggestionKind::IdleSessions)
        );
    }

    #[test]
    fn build_suggestions_all_zeros_with_active_sessions_returns_empty() {
        let result = Suggestion::build_suggestions(0, 0, &[], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn build_suggestions_message_contains_count_for_ready_issues() {
        let result = Suggestion::build_suggestions(5, 0, &[], 1);
        assert!(result[0].message.contains("5"));
    }

    #[test]
    fn build_suggestions_message_contains_percentage_for_milestone() {
        let result = Suggestion::build_suggestions(0, 0, &[("v2".to_string(), 5, 10)], 1);
        assert!(result[0].message.contains("50"));
    }

    #[test]
    fn build_suggestions_order_is_ready_then_milestone_then_failed_then_idle() {
        let milestones = vec![("v1".to_string(), 1u32, 2u32)];
        let result = Suggestion::build_suggestions(1, 1, &milestones, 0);
        assert_eq!(result.len(), 4);
        assert!(matches!(result[0].kind, SuggestionKind::ReadyIssues { .. }));
        assert!(matches!(
            result[1].kind,
            SuggestionKind::MilestoneProgress { .. }
        ));
        assert!(matches!(
            result[2].kind,
            SuggestionKind::FailedIssues { .. }
        ));
        assert_eq!(result[3].kind, SuggestionKind::IdleSessions);
    }

    // -- HomeSection focus and Tab toggle --

    #[test]
    fn home_initial_focus_section_is_quick_actions() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert_eq!(screen.focus_section, HomeSection::QuickActions);
    }

    #[test]
    fn home_tab_toggles_focus_from_quick_actions_to_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Tab));
        assert_eq!(screen.focus_section, HomeSection::Suggestions);
    }

    #[test]
    fn home_tab_toggles_focus_back_to_quick_actions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Tab));
        screen.handle_input(&key_event(KeyCode::Tab));
        assert_eq!(screen.focus_section, HomeSection::QuickActions);
    }

    // -- Suggestion list navigation --

    #[test]
    fn home_suggestions_initial_selected_index_is_zero() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_j_navigates_suggestions_when_focus_is_suggestions() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_suggestion, 1);
    }

    #[test]
    fn home_down_navigates_suggestions_when_focus_is_suggestions() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Down));
        assert_eq!(screen.selected_suggestion, 1);
    }

    #[test]
    fn home_k_navigates_suggestions_up_when_focus_is_suggestions() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_suggestion, 1);
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_suggestion_navigation_does_not_underflow() {
        let sug = Suggestion::build_suggestions(1, 0, &[], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('k')));
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_suggestion_navigation_does_not_overflow() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')));
        }
        assert_eq!(screen.selected_suggestion, 1);
    }

    #[test]
    fn home_j_navigates_quick_actions_when_focus_is_quick_actions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_action, 1);
        assert_eq!(screen.selected_suggestion, 0);
    }

    // -- Enter on a suggestion --

    #[test]
    fn home_enter_on_suggestion_returns_push_with_suggestion_action() {
        let sug = Suggestion::build_suggestions(3, 0, &[], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_enter_on_milestone_suggestion_returns_push_milestone_view() {
        let sug = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 1, 5)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_enter_on_idle_suggestion_returns_push_overview() {
        let sug = Suggestion::build_suggestions(0, 0, &[], 0);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::Push(TuiMode::Overview));
    }

    #[test]
    fn home_enter_when_suggestions_empty_and_focused_returns_none() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::None);
    }

    // -- Shortcut keys always active regardless of focus --

    #[test]
    fn home_char_i_returns_issue_browser_when_focused_on_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('i')));
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_char_m_returns_milestone_view_when_focused_on_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('m')));
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_char_q_returns_quit_when_focused_on_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('q')));
        assert_eq!(action, ScreenAction::Quit);
    }

    // -- Edge cases --

    #[test]
    fn build_suggestions_singular_message_for_one_ready_issue() {
        let result = Suggestion::build_suggestions(1, 0, &[], 1);
        assert!(result[0].message.contains("1 issue labeled"));
        assert!(!result[0].message.contains("issues"));
    }

    #[test]
    fn build_suggestions_plural_message_for_multiple_ready_issues() {
        let result = Suggestion::build_suggestions(3, 0, &[], 1);
        assert!(result[0].message.contains("3 issues"));
    }

    #[test]
    fn build_suggestions_singular_message_for_one_failed_issue() {
        let result = Suggestion::build_suggestions(0, 1, &[], 1);
        assert!(result[0].message.contains("1 issue labeled"));
        assert!(!result[0].message.contains("issues"));
    }

    #[test]
    fn build_suggestions_milestone_closed_exceeds_total_clamps_to_100() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 15, 10)], 1);
        assert!(result[0].message.contains("100%"));
    }

    #[test]
    fn build_suggestions_milestone_fully_complete_shows_100() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 10, 10)], 1);
        assert!(result[0].message.contains("100%"));
    }

    #[test]
    fn build_suggestions_milestone_zero_closed_shows_0() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 0, 5)], 1);
        assert!(result[0].message.contains("0%"));
    }

    #[test]
    fn build_suggestions_multiple_nonzero_milestones_all_emitted() {
        let milestones = vec![
            ("v1".to_string(), 1u32, 5u32),
            ("v2".to_string(), 3u32, 10u32),
            ("v3".to_string(), 7u32, 7u32),
        ];
        let result = Suggestion::build_suggestions(0, 0, &milestones, 1);
        assert_eq!(result.len(), 3);
        for (i, (title, _, _)) in milestones.iter().enumerate() {
            assert!(result[i].message.contains(title.as_str()));
        }
    }

    #[test]
    fn home_j_on_empty_suggestions_when_focused_does_not_panic() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_k_on_empty_suggestions_when_focused_does_not_panic() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn set_suggestions_resets_selected_index() {
        let sug = Suggestion::build_suggestions(1, 1, &[("v1".to_string(), 1, 2)], 0);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        // Navigate to index 2
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_suggestion, 2);
        // Replace with fewer suggestions
        let new_sug = Suggestion::build_suggestions(1, 0, &[], 1);
        screen.set_suggestions(new_sug);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_k_in_suggestions_does_not_move_quick_actions_selection() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        // Move quick actions selection to 2
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected_action, 2);
        // Switch to suggestions and navigate
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('k')));
        // Quick actions selection must be unchanged
        assert_eq!(screen.selected_action, 2);
        assert_eq!(screen.selected_suggestion, 0);
    }
}
