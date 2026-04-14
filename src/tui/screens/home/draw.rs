use super::types::{Suggestion, SuggestionKind};
use super::{HomeScreen, QUICK_ACTIONS};
use crate::changelog::{self, ChangeCategory, ChangeItem};
use crate::tui::app::TuiMode;
use crate::tui::icons::{self, IconId};
use crate::tui::screens::ScreenAction;
use crate::tui::theme::Theme;
use crate::tui::widgets::stats_bar::{StatsBar, StatsBarData};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

impl HomeScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let warning_height = if self.warnings.is_empty() {
            0
        } else {
            (self.warnings.len() as u16 + 2).min(6)
        };

        let whats_new_items = Self::whats_new_highlights();
        let whats_new_height = if whats_new_items.is_empty() {
            0
        } else {
            (whats_new_items.len() as u16 + 2).min(6)
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),                // stats bar
                Constraint::Length(warning_height),   // warnings (0 if none)
                Constraint::Length(whats_new_height), // what's new (0 if none)
                Constraint::Min(8),                   // quick actions + recent sessions
            ])
            .split(area);

        let stats_data = StatsBarData {
            loaded: self.stats.loaded,
            repo: self.project_info.repo.clone(),
            branch: self.project_info.branch.clone(),
            username: self.project_info.username.clone(),
            issues_open: self.stats.issues_open,
            issues_closed: self.stats.issues_closed,
            milestone_title: self
                .stats
                .milestone_active
                .as_ref()
                .map(|m| m.title.clone()),
            milestone_closed: self
                .stats
                .milestone_active
                .as_ref()
                .map(|m| m.closed)
                .unwrap_or(0),
            milestone_total: self
                .stats
                .milestone_active
                .as_ref()
                .map(|m| m.total)
                .unwrap_or(0),
            sessions_active: self.stats.sessions_active,
            sessions_total: self.stats.sessions_total,
        };
        StatsBar::new(stats_data, theme).render(chunks[0], f.buffer_mut());

        if !self.warnings.is_empty() {
            self.draw_warnings(f, chunks[1], theme);
        }

        if !whats_new_items.is_empty() {
            Self::draw_whats_new(f, chunks[2], theme, &whats_new_items);
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

    #[allow(dead_code)] // Reason: tick for animation/refresh — to be wired into event loop
    pub fn tick(&mut self) {
        // No-op for now; could refresh recent sessions
    }

    pub(super) fn execute_selected_action(&self) -> ScreenAction {
        match self.selected_action {
            0 => ScreenAction::Push(TuiMode::IssueBrowser),
            1 => ScreenAction::Push(TuiMode::MilestoneView),
            2 => ScreenAction::Push(TuiMode::PromptInput),
            3 => ScreenAction::Push(TuiMode::AdaptWizard),
            4 => ScreenAction::Push(TuiMode::PrReview),
            5 => ScreenAction::Push(TuiMode::Overview),
            6 => ScreenAction::Push(TuiMode::CostDashboard),
            7 => ScreenAction::Push(TuiMode::TokenDashboard),
            8 => ScreenAction::Push(TuiMode::Settings),
            9 => ScreenAction::CheckForUpdate,
            10 => ScreenAction::Quit,
            _ => ScreenAction::None,
        }
    }

    fn draw_warnings(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme
            .styled_block("Warnings", false)
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

    fn whats_new_highlights() -> Vec<(ChangeCategory, &'static ChangeItem)> {
        let version = env!("CARGO_PKG_VERSION");
        changelog::changelog().highlights_with_category(version, 4)
    }

    fn draw_whats_new(
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        items: &[(ChangeCategory, &ChangeItem)],
    ) {
        let version = env!("CARGO_PKG_VERSION");
        let whats_new_title = format!("What's New in v{}", version);
        let block = theme
            .styled_block(&whats_new_title, false)
            .title_bottom(Line::from(" Press [n] for full release notes ").centered())
            .border_style(Style::default().fg(theme.accent_info));

        let inner_width = area.width.saturating_sub(2) as usize;

        let lines: Vec<Line> = items
            .iter()
            .map(|(cat, item)| {
                let prefix = format!("  [{}] ", cat.label());
                let max_text = inner_width.saturating_sub(prefix.len());
                let text = if item.text.chars().count() > max_text {
                    let truncated: String =
                        item.text.chars().take(max_text.saturating_sub(3)).collect();
                    format!("{}...", truncated)
                } else {
                    item.text.clone()
                };
                Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(theme.accent_success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text, Style::default().fg(theme.text_primary)),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_quick_actions(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let is_focused = self.is_quick_actions_focused();
        let block = theme.styled_block("Quick Actions", is_focused);

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
        let block = theme.styled_block("Suggestions", is_focused);

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
                SuggestionKind::ReadyIssues { .. } => icons::get(IconId::IssueOpened),
                SuggestionKind::MilestoneProgress { .. } => icons::get(IconId::Milestone),
                SuggestionKind::IdleSessions => icons::get(IconId::Pause),
                SuggestionKind::FailedIssues { .. } => icons::get(IconId::XCircle),
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
        let block = theme.styled_block("Recent Activity", false);

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
                    "completed" => icons::get(IconId::CheckCircle),
                    "running" => icons::get(IconId::Play),
                    "errored" => icons::get(IconId::XCircle),
                    _ => icons::get(IconId::Hourglass),
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
