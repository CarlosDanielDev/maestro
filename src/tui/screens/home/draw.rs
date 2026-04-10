use super::types::{Suggestion, SuggestionKind};
use super::{HomeScreen, LOGO, QUICK_ACTIONS};
use crate::tui::app::TuiMode;
use crate::tui::screens::ScreenAction;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

impl HomeScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
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

        self.draw_logo(f, chunks[0], theme);
        self.draw_project_info(f, chunks[1], theme);

        if !self.warnings.is_empty() {
            self.draw_warnings(f, chunks[2], theme);
        }

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(35),
                Constraint::Percentage(35),
            ])
            .split(chunks[3]);

        self.draw_quick_actions(f, bottom[0], theme);
        self.draw_suggestions(f, bottom[1], theme);
        self.draw_recent_sessions(f, bottom[2], theme);
    }

    pub fn start_loading_suggestions(&mut self) {
        self.loading_suggestions = true;
    }

    pub fn set_suggestions(&mut self, suggestions: Vec<Suggestion>) {
        self.suggestions = suggestions;
        self.selected_suggestion = 0;
        self.loading_suggestions = false;
    }

    pub fn tick(&mut self) {
        // No-op for now; could refresh recent sessions
    }

    pub(super) fn execute_selected_action(&self) -> ScreenAction {
        match self.selected_action {
            0 => ScreenAction::Push(TuiMode::IssueBrowser),
            1 => ScreenAction::Push(TuiMode::MilestoneView),
            2 => ScreenAction::Push(TuiMode::PromptInput),
            3 => ScreenAction::Push(TuiMode::Overview),
            4 => ScreenAction::Push(TuiMode::CostDashboard),
            5 => ScreenAction::Quit,
            _ => ScreenAction::None,
        }
    }

    fn draw_logo(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let logo = Paragraph::new(LOGO)
            .style(Style::default().fg(theme.accent_success))
            .alignment(Alignment::Center);
        f.render_widget(logo, area);
    }

    fn draw_project_info(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let username_display = self.project_info.username.as_deref().unwrap_or("unknown");

        let info = Line::from(vec![
            Span::styled("  Repo: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                &self.project_info.repo,
                Style::default().fg(theme.accent_info),
            ),
            Span::raw("  |  "),
            Span::styled("Branch: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                &self.project_info.branch,
                Style::default().fg(theme.accent_warning),
            ),
            Span::raw("  |  "),
            Span::styled("User: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("@{}", username_display),
                Style::default().fg(theme.accent_success),
            ),
        ]);
        let block = Block::default().borders(Borders::BOTTOM);
        let para = Paragraph::new(info)
            .block(block)
            .alignment(Alignment::Center);
        f.render_widget(para, area);
    }

    fn draw_warnings(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" Warnings ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_warning));

        let lines: Vec<Line> = self
            .warnings
            .iter()
            .map(|w| {
                Line::from(vec![
                    Span::styled("  ! ", Style::default().fg(theme.accent_warning)),
                    Span::styled(w.as_str(), Style::default().fg(theme.text_primary)),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_quick_actions(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let is_focused = self.is_quick_actions_focused();
        let border_color = if is_focused {
            theme.border_active
        } else {
            theme.border_inactive
        };
        let block = Block::default()
            .title(" Quick Actions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let selected_style = Style::default()
            .fg(theme.branding_fg)
            .bg(theme.accent_success)
            .add_modifier(Modifier::BOLD);

        let mut lines = Vec::new();
        for (idx, (label, key)) in QUICK_ACTIONS.iter().enumerate() {
            let is_selected = is_focused && idx == self.selected_action;
            let style = if is_selected {
                selected_style
            } else {
                Style::default().fg(theme.text_primary)
            };
            let key_style = if is_selected {
                selected_style
            } else {
                Style::default().fg(theme.accent_success)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  [{}]  ", key), key_style),
                Span::styled(*label, style),
            ]));
        }

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_suggestions(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let is_focused = self.is_suggestions_focused();
        let border_color = if is_focused {
            theme.border_active
        } else {
            theme.border_inactive
        };
        let block = Block::default()
            .title(" Suggestions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.loading_suggestions {
            let para = Paragraph::new("  Loading...")
                .style(Style::default().fg(theme.accent_warning))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        if self.suggestions.is_empty() {
            let para = Paragraph::new("  No suggestions — everything looks good!")
                .style(Style::default().fg(theme.text_secondary))
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
                SuggestionKind::ReadyIssues { .. } => theme.accent_success,
                SuggestionKind::MilestoneProgress { .. } => theme.accent_info,
                SuggestionKind::IdleSessions => theme.accent_warning,
                SuggestionKind::FailedIssues { .. } => theme.accent_error,
            };
            let style = if is_selected {
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), Style::default().fg(color)),
                Span::styled(&suggestion.message, style),
            ]));
        }

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_recent_sessions(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" Recent Activity ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_inactive));

        if self.recent_sessions.is_empty() {
            let para = Paragraph::new("  No recent sessions")
                .style(Style::default().fg(theme.text_secondary))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        let lines: Vec<Line> = self
            .recent_sessions
            .iter()
            .map(|s| {
                let status_style = match s.status.as_str() {
                    "completed" => Style::default().fg(theme.accent_success),
                    "running" => Style::default().fg(theme.accent_warning),
                    "errored" => Style::default().fg(theme.accent_error),
                    _ => Style::default().fg(theme.text_secondary),
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
                        Style::default().fg(theme.accent_identifier),
                    ),
                    Span::raw(" "),
                    Span::styled(&s.title, Style::default().fg(theme.text_primary)),
                    Span::styled(
                        format!("  ${:.2}", s.cost_usd),
                        Style::default().fg(theme.text_secondary),
                    ),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }
}
