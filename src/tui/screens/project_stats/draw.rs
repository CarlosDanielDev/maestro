use super::ProjectStatsScreen;
use crate::tui::theme::Theme;
use crate::tui::widgets::EmptyState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

impl ProjectStatsScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        if self.loading {
            EmptyState::loading(
                "Project Stats",
                "Loading project stats…",
                self.spinner_tick(),
            )
            .render(f, area, theme);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(self.milestones_height()),
                Constraint::Min(3),
            ])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Project Stats",
                Style::default()
                    .fg(theme.title_accent)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            chunks[0],
        );

        self.draw_overview(f, chunks[1], theme);
        self.draw_milestones(f, chunks[2], theme);
        self.draw_recent(f, chunks[3], theme);
    }

    fn milestones_height(&self) -> u16 {
        let body = self.data.milestones.len() as u16;
        body.saturating_add(2).clamp(4, 12)
    }

    fn draw_overview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);

        self.draw_issues(f, chunks[0], theme);
        self.draw_sessions(f, chunks[1], theme);
    }

    fn draw_milestones(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Milestones", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.data.milestones.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "No open milestones.",
                    Style::default().fg(theme.text_secondary),
                ))
                .alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let rows: Vec<Line> = self
            .data
            .milestones
            .iter()
            .take(inner.height as usize)
            .map(|ms| {
                let percent = ms.percent();
                let color = theme.milestone_gauge_color(percent as f64);
                Line::from(vec![
                    Span::styled(
                        format!("{:<18}", truncate_label(&ms.title, 18)),
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:>3}% ", percent),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    progress_bar(ms.ratio(), 18, color),
                    Span::raw("  "),
                    Span::styled(
                        format!("{}/{} closed", ms.closed, ms.total),
                        Style::default().fg(theme.text_secondary),
                    ),
                ])
            })
            .collect();

        f.render_widget(Paragraph::new(rows), inner);
    }

    fn draw_issues(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let i = &self.data.issues;
        let total_tracked = i.open + i.closed;
        let closed_rate = if total_tracked == 0 {
            0.0
        } else {
            i.closed as f64 / total_tracked as f64 * 100.0
        };

        let mut backlog = vec![Span::styled(
            "Backlog   ",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )];
        backlog.extend(metric_spans("open", i.open, theme.accent_warning, theme));
        backlog.push(Span::raw("   "));
        backlog.extend(metric_spans(
            "closed",
            i.closed,
            theme.accent_success,
            theme,
        ));
        backlog.push(Span::raw("   "));
        backlog.push(Span::styled(
            format!("{closed_rate:.0}% closed"),
            Style::default().fg(if closed_rate >= 80.0 {
                theme.accent_success
            } else if closed_rate > 0.0 {
                theme.accent_warning
            } else {
                theme.text_muted
            }),
        ));

        let mut maestro = vec![Span::styled(
            "Maestro   ",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )];
        maestro.extend(metric_spans("ready", i.ready, theme.accent_info, theme));
        maestro.push(Span::raw("   "));
        maestro.extend(metric_spans("done", i.done, theme.accent_success, theme));
        maestro.push(Span::raw("   "));
        maestro.extend(metric_spans("failed", i.failed, theme.accent_error, theme));

        let lines = vec![Line::from(backlog), Line::from(maestro)];

        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Issues", false)),
            area,
        );
    }

    fn draw_sessions(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let m = &self.data.sessions;
        let success_rate = m.success_rate() * 100.0;
        let success_color = if success_rate >= 80.0 {
            theme.accent_success
        } else if success_rate > 0.0 {
            theme.accent_warning
        } else {
            theme.text_muted
        };

        let mut runs = vec![Span::styled(
            "Runs      ",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )];
        runs.extend(metric_spans(
            "total",
            m.total_sessions,
            theme.text_primary,
            theme,
        ));
        runs.push(Span::raw("   "));
        runs.extend(metric_spans(
            "complete",
            m.completed_sessions,
            theme.accent_success,
            theme,
        ));
        runs.push(Span::raw("   "));
        runs.push(Span::styled(
            format!("{success_rate:.0}% success"),
            Style::default().fg(success_color),
        ));

        let spend = vec![
            Span::styled(
                "Spend     ",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("${:.4}", m.total_cost_usd),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("tokens ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!(
                    "{} in / {} out",
                    m.total_input_tokens, m.total_output_tokens
                ),
                Style::default().fg(theme.accent_info),
            ),
        ];

        let lines = vec![Line::from(runs), Line::from(spend)];

        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Sessions", false)),
            area,
        );
    }

    fn draw_recent(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Recent activity", false);
        let inner = block.inner(area);

        if self.data.recent_activity.is_empty() {
            EmptyState::idle(
                "Recent activity",
                "No recent sessions.",
                "Press [r] to launch one.",
            )
            .render(f, area, theme);
            return;
        }

        f.render_widget(block, area);
        let visible_height = inner.height as usize;
        let rows: Vec<Line> = self
            .data
            .recent_activity
            .iter()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|r| {
                let issue = r
                    .issue_number
                    .map(|n| format!("#{:<5}", n))
                    .unwrap_or_else(|| "      ".to_string());
                Line::from(vec![
                    Span::styled(issue, Style::default().fg(theme.accent_identifier)),
                    Span::styled(
                        format!("{:<32}", truncate_label(&r.label, 32)),
                        Style::default().fg(theme.text_primary),
                    ),
                    Span::styled(
                        format!("{:<10}", truncate_label(&r.status, 10)),
                        Style::default().fg(theme.accent_success),
                    ),
                    Span::styled(
                        format!("${:<7.4}", r.cost_usd),
                        Style::default().fg(theme.accent_warning),
                    ),
                    Span::styled(r.elapsed.clone(), Style::default().fg(theme.text_secondary)),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(rows), inner);
    }
}

fn progress_bar<'a>(ratio: f64, width: usize, color: Color) -> Span<'a> {
    let clamped = ratio.clamp(0.0, 1.0);
    let filled = (clamped * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    Span::styled(
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty)),
        Style::default().fg(color),
    )
}

fn metric_spans<'a>(
    label: &'a str,
    value: impl std::fmt::Display,
    value_color: Color,
    theme: &Theme,
) -> Vec<Span<'a>> {
    vec![
        Span::styled(label, Style::default().fg(theme.text_secondary)),
        Span::raw(" "),
        Span::styled(
            value.to_string(),
            Style::default()
                .fg(value_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]
}

fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
