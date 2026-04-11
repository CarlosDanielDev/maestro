use crate::session::types::{Session, TokenUsage};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Format a token count for display (e.g., "245.0k", "1.2M").
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Draw the token dashboard view showing token consumption analytics.
pub fn draw_token_dashboard(
    f: &mut Frame,
    sessions: &[&Session],
    total_cost: f64,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // aggregate stats
            Constraint::Min(5),    // per-session breakdown
        ])
        .split(area);

    draw_aggregate_stats(f, sessions, total_cost, chunks[0], theme);
    draw_session_tokens(f, sessions, chunks[1], theme);
}

fn draw_aggregate_stats(
    f: &mut Frame,
    sessions: &[&Session],
    total_cost: f64,
    area: Rect,
    theme: &Theme,
) {
    let mut aggregate = TokenUsage::default();
    for s in sessions {
        aggregate.accumulate(&s.token_usage);
    }

    let lines = vec![
        Line::from(vec![
            Span::styled(" Input: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(aggregate.input_tokens),
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Output: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(aggregate.output_tokens),
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Cache Read: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(aggregate.cache_read_tokens),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled("  Cache Write: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(aggregate.cache_creation_tokens),
                Style::default().fg(theme.accent_warning),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Total: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(aggregate.total_tokens()),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Cache Hit: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{:.1}%", aggregate.cache_hit_ratio() * 100.0),
                Style::default().fg(if aggregate.cache_hit_ratio() > 0.5 {
                    theme.accent_success
                } else {
                    theme.accent_warning
                }),
            ),
            Span::styled(
                "  Output Ratio: ",
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format!("{:.1}%", aggregate.output_ratio() * 100.0),
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Cost/kTok: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("${:.4}", aggregate.cost_per_kilo_token(total_cost)),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(
                format!("  Sessions: {}", sessions.len()),
                Style::default().fg(theme.text_secondary),
            ),
        ]),
    ];

    let block = theme
        .styled_block("Aggregate Token Usage", false)
        .border_style(Style::default().fg(theme.accent_info));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_session_tokens(f: &mut Frame, sessions: &[&Session], area: Rect, theme: &Theme) {
    let mut sorted: Vec<&Session> = sessions.to_vec();
    sorted.sort_by(|a, b| {
        b.token_usage
            .total_tokens()
            .cmp(&a.token_usage.total_tokens())
    });

    let header = Line::from(vec![Span::styled(
        format!(
            " {:>8}  {:>18}  {:>8}  {:>8}  {:>8}  {:>8}  {:>6}",
            "ID", "Title", "Input", "Output", "Cache R", "Cache W", "$/kT"
        ),
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD),
    )]);

    let max_rows = area.height.saturating_sub(3) as usize;
    let mut lines = vec![header];

    for s in sorted.iter().take(max_rows) {
        let label = match s.issue_number {
            Some(n) => format!("#{}", n),
            None => format!("S-{}", &s.id.to_string()[..8]),
        };
        let title: String = s
            .issue_title
            .as_deref()
            .unwrap_or(&s.prompt[..s.prompt.len().min(18)])
            .chars()
            .take(18)
            .collect();
        let cost_per_k = s.token_usage.cost_per_kilo_token(s.cost_usd);

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:>8}", label),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled(
                format!("  {:>18}", title),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                format!("  {:>8}", format_tokens(s.token_usage.input_tokens)),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                format!("  {:>8}", format_tokens(s.token_usage.output_tokens)),
                Style::default().fg(theme.accent_success),
            ),
            Span::styled(
                format!("  {:>8}", format_tokens(s.token_usage.cache_read_tokens)),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled(
                format!(
                    "  {:>8}",
                    format_tokens(s.token_usage.cache_creation_tokens)
                ),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(
                format!("  ${:.3}", cost_per_k),
                Style::default().fg(theme.accent_warning),
            ),
        ]));
    }

    let block = theme.styled_block("Per-Session Token Breakdown", false);

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tokens_small_numbers() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(45000), "45.0k");
    }

    #[test]
    fn format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }
}
