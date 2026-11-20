use crate::budget::sanitize::sanitize_cost;
use crate::session::types::{Session, SessionStatus};
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::money::money;
use crate::tui::screens::landing::draw::ellipsize;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, Paragraph},
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
    // `ds::money` never prints `$-0.00` (#1399); the percent uses the same
    // clean value so it never reads `-0%`.
    let total_cost = sanitize_cost(total_cost);
    let (ratio, label) = match budget_limit {
        Some(limit) if limit > 0.0 => {
            let pct = (total_cost / limit * 100.0).min(100.0);
            let ratio = (total_cost / limit).min(1.0);
            (
                ratio,
                format!(
                    "{} / {} ({:.0}%)",
                    money(Some(total_cost)),
                    money(Some(limit)),
                    pct
                ),
            )
        }
        _ => (
            0.0,
            format!("{} spent (no budget limit)", money(Some(total_cost))),
        ),
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
            theme
                .styled_block("Budget", false)
                .border_style(Style::default().fg(color)),
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

    let block = theme.styled_block_plain(false);

    f.render_widget(Paragraph::new(stats).block(block), area);
}

/// Widest Title column; it shrinks so the header fits the panel (#1399).
const TITLE_W: usize = 45;
/// Header columns other than Title: indent, ID, the four separators and
/// Cost / Status / Elapsed.
const FIXED_W: usize = 2 + 12 + 4 + 30;

fn draw_session_costs(f: &mut Frame, sessions: &[&Session], area: Rect, theme: &Theme) {
    let block = theme.styled_block("Session Costs", false);
    let inner_w = block.inner(area).width as usize;
    let title_w = inner_w.saturating_sub(FIXED_W).min(TITLE_W);
    let mut lines: Vec<Line> = vec![Line::from(vec![Span::styled(
        format!(
            "  {:<12} {:<title_w$} {:>10} {:>10} {:>10}",
            "ID", "Title", "Cost", "Status", "Elapsed"
        ),
        Style::default()
            .fg(theme.accent_info)
            .add_modifier(Modifier::BOLD),
    )])];

    lines.push(Line::from(Span::styled(
        format!("  {}", "─".repeat(FIXED_W - 2 + title_w)),
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
        let title_display: String = if title_w < TITLE_W {
            ellipsize(title, title_w)
        } else if title.chars().count() > 43 {
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
                format!("{:<title_w$}", title_display),
                Style::default().fg(theme.text_primary),
            ),
            // Same separators as the header, so the columns line up and a
            // 12-letter status never runs into the cost (#1399).
            Span::styled(
                format!(" {:>10}", session.cost_display()),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(
                format!(" {:>10}", ellipsize(session.status.label(), 10)),
                Style::default().fg(status_color),
            ),
            Span::styled(
                format!(" {:>10}", session.elapsed_display()),
                Style::default().fg(theme.text_secondary),
            ),
        ]));
    }

    let lines: Vec<Line> = lines
        .into_iter()
        .map(|l| ellipsize_line(l, inner_w))
        .collect();
    let paragraph = Paragraph::new(lines).block(block);

    f.render_widget(paragraph, area);
}
