use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;

/// Data for the compact stats bar widget.
#[derive(Debug, Clone)]
pub struct StatsBarData {
    pub loaded: bool,
    pub repo: String,
    pub branch: String,
    pub username: Option<String>,
    pub issues_open: usize,
    pub issues_closed: usize,
    pub milestone_title: Option<String>,
    pub milestone_closed: u32,
    pub milestone_total: u32,
    pub sessions_active: usize,
    pub sessions_total: usize,
}

/// Compact project stats bar widget replacing the large header brand.
pub struct StatsBar<'a> {
    data: StatsBarData,
    theme: &'a Theme,
}

impl<'a> StatsBar<'a> {
    pub fn new(data: StatsBarData, theme: &'a Theme) -> Self {
        Self { data, theme }
    }

    fn build_line(&self) -> Line {
        let username = self.data.username.as_deref().unwrap_or("unknown");

        let mut spans = vec![
            // Repo info section
            Span::styled(
                format!(" {} ", icons::get(IconId::Repo)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(&self.data.repo, Style::default().fg(self.theme.accent_info)),
            Span::styled(
                format!("  {} ", icons::get(IconId::Branch)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                &self.data.branch,
                Style::default().fg(self.theme.accent_warning),
            ),
            Span::styled(
                format!("  {} ", icons::get(IconId::User)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                format!("@{}", username),
                Style::default().fg(self.theme.accent_success),
            ),
            Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
        ];

        if !self.data.loaded {
            spans.push(Span::styled(
                "Loading...",
                Style::default().fg(self.theme.accent_warning),
            ));
            return Line::from(spans);
        }

        // Issues section
        let total_issues = self.data.issues_open + self.data.issues_closed;
        spans.extend([
            Span::styled(
                format!("{} ", icons::get(IconId::IssueOpened)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                self.data.issues_open.to_string(),
                Style::default()
                    .fg(self.theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" open ", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                format!("{} ", icons::get(IconId::CheckCircle)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                self.data.issues_closed.to_string(),
                Style::default()
                    .fg(self.theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" closed ({})", total_issues),
                Style::default().fg(self.theme.text_secondary),
            ),
        ]);

        // Milestone section
        if let Some(ref title) = self.data.milestone_title {
            let pct = if self.data.milestone_total > 0 {
                (self.data.milestone_closed as f64 / self.data.milestone_total as f64) * 100.0
            } else {
                0.0
            };
            let bar_width = 8usize;
            let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
            let empty = bar_width.saturating_sub(filled);

            spans.extend([
                Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    format!("{} ", icons::get(IconId::Milestone)),
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(title, Style::default().fg(self.theme.accent_info)),
                Span::raw(" "),
                Span::styled(
                    icons::get(IconId::GaugeFilled).repeat(filled),
                    Style::default().fg(self.theme.accent_success),
                ),
                Span::styled(
                    icons::get(IconId::GaugeEmpty).repeat(empty),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(
                    format!(" {:.0}%", pct),
                    Style::default().fg(self.theme.accent_success),
                ),
            ]);
        }

        // Sessions section
        spans.extend([
            Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
            Span::styled(
                format!("{} ", icons::get(IconId::Agents)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                self.data.sessions_active.to_string(),
                Style::default()
                    .fg(self.theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" active / {} total", self.data.sessions_total),
                Style::default().fg(self.theme.text_secondary),
            ),
        ]);

        Line::from(spans)
    }
}

impl<'a> Widget for StatsBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 2 {
            return;
        }

        let block = crate::tui::theme::Theme::stats_block(self.theme);
        let inner = block.inner(area);
        block.render(area, buf);

        let line = self.build_line();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(inner);

        if !chunks.is_empty() {
            Paragraph::new(line).render(chunks[0], buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_to_string(data: StatsBarData, width: u16, height: u16) -> String {
        let theme = Theme::default();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(StatsBar::new(data, &theme), f.area());
            })
            .unwrap();
        format!("{:?}", terminal.backend())
    }

    fn make_loaded_data() -> StatsBarData {
        StatsBarData {
            loaded: true,
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some("carlos".to_string()),
            issues_open: 12,
            issues_closed: 45,
            milestone_title: Some("v1.0".to_string()),
            milestone_closed: 7,
            milestone_total: 10,
            sessions_active: 2,
            sessions_total: 8,
        }
    }

    #[test]
    fn renders_repo_info() {
        let out = render_to_string(make_loaded_data(), 150, 3);
        assert!(out.contains("owner/repo"), "repo must appear");
        assert!(out.contains("main"), "branch must appear");
        assert!(out.contains("carlos"), "username must appear");
    }

    #[test]
    fn renders_issue_counts() {
        let out = render_to_string(make_loaded_data(), 150, 3);
        assert!(out.contains("12"), "open issue count must appear");
        assert!(out.contains("45"), "closed issue count must appear");
    }

    #[test]
    fn renders_milestone_progress() {
        let out = render_to_string(make_loaded_data(), 150, 3);
        assert!(out.contains("v1.0"), "milestone title must appear");
        assert!(out.contains("70%"), "milestone percentage must appear");
    }

    #[test]
    fn renders_session_counts() {
        let out = render_to_string(make_loaded_data(), 150, 3);
        assert!(out.contains("2"), "active sessions must appear");
        assert!(out.contains("8"), "total sessions must appear");
    }

    #[test]
    fn renders_loading_when_not_loaded() {
        let mut data = make_loaded_data();
        data.loaded = false;
        let out = render_to_string(data, 150, 3);
        assert!(out.contains("Loading"), "must show loading indicator");
    }

    #[test]
    fn handles_no_milestone() {
        let mut data = make_loaded_data();
        data.milestone_title = None;
        let out = render_to_string(data, 150, 3);
        assert!(out.contains("owner/repo"), "repo must still appear");
    }

    #[test]
    fn renders_without_panic_at_minimum_size() {
        let _ = render_to_string(make_loaded_data(), 1, 1);
    }
}
