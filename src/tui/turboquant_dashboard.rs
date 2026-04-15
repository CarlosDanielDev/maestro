use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::session::types::Session;
use crate::tui::theme::Theme;
use crate::tui::token_dashboard::format_tokens;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Draw the TurboQuant A/B benchmark dashboard.
/// Shows side-by-side comparison of sessions with TQ enabled vs disabled.
pub fn draw_turboquant_dashboard(
    f: &mut Frame,
    sessions: &[&Session],
    flags: &FeatureFlags,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // compression metrics summary
            Constraint::Length(7), // A/B comparison
            Constraint::Min(5),    // per-session breakdown
        ])
        .split(area);

    draw_compression_summary(f, sessions, flags, chunks[0], theme);
    draw_ab_comparison(f, sessions, chunks[1], theme);
    draw_session_breakdown(f, sessions, chunks[2], theme);
}

fn draw_compression_summary(
    f: &mut Frame,
    sessions: &[&Session],
    flags: &FeatureFlags,
    area: Rect,
    theme: &Theme,
) {
    let tq_enabled = flags.is_enabled(Flag::TurboQuant);
    let status_text = if tq_enabled { "ON" } else { "OFF" };
    let status_color = if tq_enabled {
        theme.accent_success
    } else {
        theme.text_muted
    };

    // Aggregate TQ metrics from sessions
    let (total_original, total_compressed, session_count) = aggregate_tq_metrics(sessions);

    let ratio = if total_compressed > 0 {
        total_original as f64 / total_compressed as f64
    } else {
        0.0
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(" TurboQuant: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                status_text,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  Sessions with TQ data: {}", session_count),
                Style::default().fg(theme.text_secondary),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " Original tokens: ",
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format_tokens(total_original),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled("  Compressed: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(total_compressed),
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Ratio: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                if ratio > 0.0 {
                    format!("{:.1}x", ratio)
                } else {
                    "N/A".to_string()
                },
                Style::default().fg(theme.accent_warning),
            ),
        ]),
    ];

    let block = theme
        .styled_block("TurboQuant Compression Metrics", false)
        .border_style(Style::default().fg(theme.accent_info));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_ab_comparison(f: &mut Frame, sessions: &[&Session], area: Rect, theme: &Theme) {
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Split sessions by TQ status
    let (tq_off, tq_on) = partition_sessions(sessions);

    // Left panel: TQ OFF baseline
    let off_stats = compute_panel_stats(&tq_off);
    let off_lines = vec![
        Line::from(vec![Span::styled(
            format!(" Sessions: {}", tq_off.len()),
            Style::default().fg(theme.text_secondary),
        )]),
        Line::from(vec![
            Span::styled(
                " Avg tokens/session: ",
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format_tokens(off_stats.avg_tokens),
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Total cost: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("${:.2}", off_stats.total_cost),
                Style::default().fg(theme.accent_warning),
            ),
        ]),
    ];

    let off_block = theme
        .styled_block("TurboQuant OFF (Baseline)", false)
        .border_style(Style::default().fg(theme.text_muted));
    f.render_widget(Paragraph::new(off_lines).block(off_block), halves[0]);

    // Right panel: TQ ON
    let on_stats = compute_panel_stats(&tq_on);
    let token_delta = if off_stats.avg_tokens > 0 && on_stats.avg_tokens > 0 {
        let saved_pct = (1.0 - on_stats.avg_tokens as f64 / off_stats.avg_tokens as f64) * 100.0;
        format!(" (↓{:.0}%)", saved_pct.max(0.0))
    } else {
        String::new()
    };
    let cost_delta = if off_stats.total_cost > 0.0 && on_stats.total_cost > 0.0 {
        let saved_pct = (1.0 - on_stats.total_cost / off_stats.total_cost) * 100.0;
        format!(" (↓{:.0}%)", saved_pct.max(0.0))
    } else {
        String::new()
    };

    let on_lines = vec![
        Line::from(vec![Span::styled(
            format!(" Sessions: {}", tq_on.len()),
            Style::default().fg(theme.text_secondary),
        )]),
        Line::from(vec![
            Span::styled(
                " Avg tokens/session: ",
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format_tokens(on_stats.avg_tokens),
                Style::default().fg(theme.accent_success),
            ),
            Span::styled(token_delta, Style::default().fg(theme.accent_success)),
        ]),
        Line::from(vec![
            Span::styled(" Total cost: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("${:.2}", on_stats.total_cost),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(cost_delta, Style::default().fg(theme.accent_success)),
        ]),
    ];

    let on_block = theme
        .styled_block("TurboQuant ON", false)
        .border_style(Style::default().fg(theme.accent_success));
    f.render_widget(Paragraph::new(on_lines).block(on_block), halves[1]);
}

fn draw_session_breakdown(f: &mut Frame, sessions: &[&Session], area: Rect, theme: &Theme) {
    if sessions.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            " No sessions yet. Run sessions with TurboQuant on and off to compare.",
            Style::default().fg(theme.text_muted),
        )))
        .block(theme.styled_block("Session Breakdown", false));
        f.render_widget(empty, area);
        return;
    }

    let header = Line::from(vec![Span::styled(
        format!(
            " {:>8}  {:>18}  {:>8}  {:>8}  {:>10}  {:>4}",
            "ID", "Title", "Input", "Output", "Cost", "TQ"
        ),
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD),
    )]);

    let max_rows = area.height.saturating_sub(3) as usize;
    let mut lines = vec![header];

    for s in sessions.iter().take(max_rows) {
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

        let tq_label = if s.tq_compressed_tokens.is_some() {
            "ON"
        } else {
            "OFF"
        };
        let tq_color = if s.tq_compressed_tokens.is_some() {
            theme.accent_success
        } else {
            theme.text_muted
        };

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
                format!("  {:>10}", format!("${:.2}", s.cost_usd)),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(format!("  {:>4}", tq_label), Style::default().fg(tq_color)),
        ]));
    }

    let block = theme.styled_block("Per-Session Breakdown", false);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Aggregate TQ compression metrics across sessions.
pub fn aggregate_tq_metrics(sessions: &[&Session]) -> (u64, u64, usize) {
    let mut total_original = 0u64;
    let mut total_compressed = 0u64;
    let mut count = 0;

    for s in sessions {
        if let Some(compressed) = s.tq_compressed_tokens {
            total_original += s.tq_original_tokens.unwrap_or(0);
            total_compressed += compressed;
            count += 1;
        }
    }

    (total_original, total_compressed, count)
}

struct PanelStats {
    avg_tokens: u64,
    total_cost: f64,
}

/// Partition sessions into TQ-off and TQ-on groups.
fn partition_sessions<'a>(sessions: &[&'a Session]) -> (Vec<&'a Session>, Vec<&'a Session>) {
    let mut off = Vec::new();
    let mut on = Vec::new();
    for &s in sessions {
        if s.tq_compressed_tokens.is_some() {
            on.push(s);
        } else {
            off.push(s);
        }
    }
    (off, on)
}

fn compute_panel_stats(sessions: &[&Session]) -> PanelStats {
    if sessions.is_empty() {
        return PanelStats {
            avg_tokens: 0,
            total_cost: 0.0,
        };
    }
    let total_tokens: u64 = sessions.iter().map(|s| s.token_usage.total_tokens()).sum();
    let total_cost: f64 = sessions.iter().map(|s| s.cost_usd).sum();
    PanelStats {
        avg_tokens: total_tokens / sessions.len() as u64,
        total_cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_tq_metrics_empty() {
        let (orig, comp, count) = aggregate_tq_metrics(&[]);
        assert_eq!(orig, 0);
        assert_eq!(comp, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn partition_sessions_empty() {
        let (off, on) = partition_sessions(&[]);
        assert!(off.is_empty());
        assert!(on.is_empty());
    }

    #[test]
    fn compute_panel_stats_empty() {
        let stats = compute_panel_stats(&[]);
        assert_eq!(stats.avg_tokens, 0);
        assert_eq!(stats.total_cost, 0.0);
    }
}
