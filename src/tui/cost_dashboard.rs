use crate::session::types::{Session, SessionStatus};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
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
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // budget gauge
            Constraint::Length(3), // summary stats
            Constraint::Min(5),    // per-session breakdown
        ])
        .split(area);

    draw_budget_gauge(f, total_cost, budget_limit, chunks[0]);
    draw_summary_stats(f, sessions, total_cost, chunks[1]);
    draw_session_costs(f, sessions, chunks[2]);
}

fn draw_budget_gauge(f: &mut Frame, total_cost: f64, budget_limit: Option<f64>, area: Rect) {
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
        Color::Red
    } else if ratio >= 0.7 {
        Color::Yellow
    } else {
        Color::Green
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .title(" Budget ")
                .title_alignment(Alignment::Center),
        )
        .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(gauge, area);
}

fn draw_summary_stats(f: &mut Frame, sessions: &[&Session], total_cost: f64, area: Rect) {
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
        Span::styled(" Sessions: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            sessions.len().to_string(),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} running", running),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} completed", completed),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} errored", errored),
            Style::default().fg(Color::Red),
        ),
        Span::raw("    "),
        Span::styled(" Avg cost: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("${:.2}", avg_cost),
            Style::default().fg(Color::Yellow),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    f.render_widget(Paragraph::new(stats).block(block), area);
}

fn draw_session_costs(f: &mut Frame, sessions: &[&Session], area: Rect) {
    let mut lines: Vec<Line> = vec![Line::from(vec![Span::styled(
        format!(
            "  {:<12} {:<45} {:>10} {:>10} {:>10}",
            "ID", "Title", "Cost", "Status", "Elapsed"
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )])];

    lines.push(Line::from(Span::styled(
        format!("  {}", "─".repeat(89)),
        Style::default().fg(Color::DarkGray),
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

        let status_color = match session.status {
            SessionStatus::Running => Color::Green,
            SessionStatus::Completed => Color::Cyan,
            SessionStatus::Errored => Color::Red,
            _ => Color::White,
        };

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{:<12}", id_str), Style::default().fg(Color::White)),
            Span::raw(" "),
            Span::styled(
                format!("{:<45}", title_display),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:>10}", format!("${:.2}", session.cost_usd)),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:>10}", session.status.label()),
                Style::default().fg(status_color),
            ),
            Span::styled(
                format!("{:>10}", session.elapsed_display()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Session Costs ")
            .title_alignment(Alignment::Center),
    );

    f.render_widget(paragraph, area);
}
