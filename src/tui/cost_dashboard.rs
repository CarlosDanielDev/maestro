use crate::session::types::{Session, SessionStatus};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};

/// Draw the cost dashboard view showing spending breakdown.
pub fn draw_cost_dashboard(
    f: &mut Frame,
    sessions: &[&Session],
    total_cost: f64,
    budget_limit: Option<f64>,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // budget gauge
            Constraint::Length(3), // summary stats
            Constraint::Min(5),    // per-session breakdown
        ])
        .split(area);

    draw_budget_gauge(f, total_cost, budget_limit, chunks[0], theme);
    draw_summary_stats(f, sessions, total_cost, chunks[1], theme);
    draw_session_costs(f, sessions, chunks[2], theme);
}

fn draw_budget_gauge(
    f: &mut Frame,
    total_cost: f64,
    budget_limit: Option<f64>,
    area: Rect,
    theme: &Theme,
) {
    let (ratio, label) = match budget_limit {
        Some(limit) if limit > 0.0 => {
            let pct = (total_cost / limit * 100.0).min(100.0);
            let ratio = (total_cost / limit).min(1.0);
            (
                ratio,
                format!("${:.2} / ${:.2} ({:.0}%)", total_cost, limit, pct),
            )
        }
        _ => (0.0, format!("${:.2} spent (no budget limit)", total_cost)),
    };

    let color = if ratio >= 0.9 {
        theme.gauge_high
    } else if ratio >= 0.7 {
        theme.gauge_medium
    } else {
        theme.gauge_low
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .title(" Budget ")
                .title_alignment(Alignment::Center),
        )
        .gauge_style(Style::default().fg(color).bg(theme.gauge_background))
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(gauge, area);
}

fn draw_summary_stats(
    f: &mut Frame,
    sessions: &[&Session],
    total_cost: f64,
    area: Rect,
    theme: &Theme,
) {
    let completed = sessions
        .iter()
        .filter(|s| s.status == SessionStatus::Completed)
        .count();
    let errored = sessions
        .iter()
        .filter(|s| s.status == SessionStatus::Errored)
        .count();
    let running = sessions
        .iter()
        .filter(|s| s.status == SessionStatus::Running)
        .count();
    let avg_cost = if !sessions.is_empty() {
        total_cost / sessions.len() as f64
    } else {
        0.0
    };

    let stats = Line::from(vec![
        Span::styled(" Sessions: ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            sessions.len().to_string(),
            Style::default().fg(theme.text_primary),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} running", running),
            Style::default().fg(theme.accent_success),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} completed", completed),
            Style::default().fg(theme.accent_info),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} errored", errored),
            Style::default().fg(theme.accent_error),
        ),
        Span::raw("    "),
        Span::styled(" Avg cost: ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            format!("${:.2}", avg_cost),
            Style::default().fg(theme.accent_warning),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_inactive));

    f.render_widget(Paragraph::new(stats).block(block), area);
}

fn draw_session_costs(f: &mut Frame, sessions: &[&Session], area: Rect, theme: &Theme) {
    let mut lines: Vec<Line> = vec![Line::from(vec![Span::styled(
        format!(
            "  {:<12} {:<45} {:>10} {:>10} {:>10}",
            "ID", "Title", "Cost", "Status", "Elapsed"
        ),
        Style::default()
            .fg(theme.accent_info)
            .add_modifier(Modifier::BOLD),
    )])];

    lines.push(Line::from(Span::styled(
        format!("  {}", "─".repeat(89)),
        Style::default().fg(theme.border_inactive),
    )));

    // Sort by cost descending
    let mut sorted: Vec<&&Session> = sessions.iter().collect();
    sorted.sort_by(|a, b| {
        b.cost_usd
            .partial_cmp(&a.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for session in sorted {
        let id_str = match session.issue_number {
            Some(n) => format!("#{}", n),
            None => session.id.to_string()[..8].to_string(),
        };

        let title = session.issue_title.as_deref().unwrap_or(&session.prompt);
        let title_display: String = if title.chars().count() > 43 {
            let truncated: String = title.chars().take(40).collect();
            format!("{}...", truncated)
        } else {
            title.to_string()
        };

        let status_color = theme.status_color(session.status);

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<12}", id_str),
                Style::default().fg(theme.text_primary),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:<45}", title_display),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                format!("{:>10}", format!("${:.2}", session.cost_usd)),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(
                format!("{:>10}", session.status.label()),
                Style::default().fg(status_color),
            ),
            Span::styled(
                format!("{:>10}", session.elapsed_display()),
                Style::default().fg(theme.text_secondary),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_inactive))
            .title(" Session Costs ")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(paragraph, area);
}
