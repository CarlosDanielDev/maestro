use crate::budget::quota_snapshot::{ProviderQuotaSnapshots, QuotaBucket, QuotaRow};
use crate::budget::sanitize::sanitize_cost;
use crate::session::types::{Session, TokenUsage};
use crate::tui::theme::Theme;
use crate::util::formatting::format_tokens;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub mod provider_rollup;

use provider_rollup::{ProviderRow, build_provider_rows, provider_context_window};

/// Zero-quota source used by the public `draw_token_dashboard` entry point
/// when no `App.minimax_quota` is wired. Production now picks between this
/// and `MinimaxQuotaSnapshots` at the App layer (#769). Tests inject their
/// own fake via `draw_token_dashboard_with_quota`.
struct NoQuotaSnapshots;

impl ProviderQuotaSnapshots for NoQuotaSnapshots {
    fn quota_for(&self, _provider_id: &str) -> Option<QuotaRow> {
        None
    }
}

/// Draw the token dashboard view showing per-provider rollup + aggregate +
/// per-session breakdown. Public entry point used when no live quota
/// source is available; passes `NoQuotaSnapshots` so quota cells render
/// as `-`. The dashboard command's wire-up picks the live variant via
/// `draw_token_dashboard_with_quota` (#769).
pub fn draw_token_dashboard(
    f: &mut Frame,
    sessions: &[&Session],
    total_cost: f64,
    area: Rect,
    theme: &Theme,
) {
    draw_token_dashboard_with_quota(f, sessions, total_cost, &NoQuotaSnapshots, area, theme);
}

/// Inner draw that takes an injected `ProviderQuotaSnapshots`. Snapshot tests
/// use this to assert the quota column rendering without depending on a real
/// `MinimaxQuota`. The public wrapper above passes `NoQuotaSnapshots`.
pub fn draw_token_dashboard_with_quota(
    f: &mut Frame,
    sessions: &[&Session],
    total_cost: f64,
    quota_snapshots: &dyn ProviderQuotaSnapshots,
    area: Rect,
    theme: &Theme,
) {
    let rows = build_provider_rows(sessions, quota_snapshots, provider_context_window);
    // header + N rows + 2 border lines. Minimum 4 lines so the empty-state
    // placeholder is visible inside the styled_block.
    let row_lines = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    let rollup_height = row_lines.saturating_add(3).max(4);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(rollup_height),
            Constraint::Length(5), // aggregate stats condensed to 3 content lines
            Constraint::Min(5),    // per-session breakdown
        ])
        .split(area);

    draw_provider_rollup(f, &rows, chunks[0], theme);
    draw_aggregate_stats(f, sessions, total_cost, chunks[1], theme);
    draw_session_tokens(f, sessions, chunks[2], theme);
}

fn draw_provider_rollup(f: &mut Frame, rows: &[ProviderRow], area: Rect, theme: &Theme) {
    let narrow = area.width < 100;
    let mut lines: Vec<Line> = Vec::with_capacity(rows.len() + 1);
    let header_text = if narrow {
        format!(
            " {:<10}{:>10}{:>14}{:>16}",
            "Provider", "Cost", "Ctx", "Quota"
        )
    } else {
        format!(
            " {:<10}{:>10}{:>26}{:>26}",
            "Provider", "Cost", "Context", "Quota"
        )
    };
    lines.push(Line::from(vec![Span::styled(
        header_text,
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD),
    )]));

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            " (no active sessions)".to_string(),
            Style::default().fg(theme.text_secondary),
        )));
    } else {
        for row in rows {
            lines.push(row_line(row, narrow, theme));
        }
    }

    let block = theme.styled_block("Per-Provider Usage", false);
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn row_line(row: &ProviderRow, narrow: bool, theme: &Theme) -> Line<'static> {
    let cost_text = format!("${:.2}", row.total_cost_usd);
    let ctx_text = format_context_cell(row.context_used, row.context_window, narrow);
    let quota_text = format_quota_cell(row.quota, narrow);
    let quota_style = match row.quota.map(|q| q.status) {
        Some(QuotaBucket::Refused) => Style::default().fg(theme.gauge_high),
        Some(QuotaBucket::Warn) => Style::default().fg(theme.gauge_medium),
        Some(QuotaBucket::Ok) => Style::default().fg(theme.gauge_low),
        None => Style::default().fg(theme.text_secondary),
    };

    let provider_cell = if narrow {
        format!(" {:<10}", truncate_provider(&row.provider_id, 10))
    } else {
        format!(" {:<10}", row.provider_id)
    };
    let cost_cell = format!("{:>10}", cost_text);
    let (ctx_width, quota_width) = if narrow { (14, 16) } else { (26, 26) };
    let ctx_cell = format!("{:>w$}", ctx_text, w = ctx_width);
    let quota_cell = format!("{:>w$}", quota_text, w = quota_width);

    Line::from(vec![
        Span::styled(provider_cell, Style::default().fg(theme.text_primary)),
        Span::styled(cost_cell, Style::default().fg(theme.accent_warning)),
        Span::styled(ctx_cell, Style::default().fg(theme.accent_info)),
        Span::styled(quota_cell, quota_style),
    ])
}

fn truncate_provider(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn format_context_cell(used: u64, window: Option<u32>, narrow: bool) -> String {
    match window {
        Some(w) if narrow => format!("{}/{}", format_tokens(used), format_tokens(u64::from(w))),
        Some(w) => format!(
            "ctx {}/{}",
            with_thousands(used),
            with_thousands(u64::from(w))
        ),
        None if narrow => format_tokens(used),
        None => format!("ctx {}", with_thousands(used)),
    }
}

fn format_quota_cell(quota: Option<QuotaRow>, narrow: bool) -> String {
    match quota {
        None => "-".to_string(),
        Some(q) if narrow => format!("{}/{}", q.used, q.limit),
        Some(q) => format!("quota {}/{} ({})", q.used, q.limit, q.window_label),
    }
}

fn with_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn draw_aggregate_stats(
    f: &mut Frame,
    sessions: &[&Session],
    _total_cost: f64,
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
            Span::styled("  Cache R: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(aggregate.cache_read_tokens),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled("  Cache W: ", Style::default().fg(theme.text_secondary)),
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
        let cost_per_k = s.token_usage.cost_per_kilo_token(sanitize_cost(s.cost_usd));

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
    fn with_thousands_handles_small_numbers() {
        assert_eq!(with_thousands(0), "0");
        assert_eq!(with_thousands(999), "999");
    }

    #[test]
    fn with_thousands_inserts_separator_at_thousands() {
        assert_eq!(with_thousands(1_000), "1,000");
        assert_eq!(with_thousands(12_304), "12,304");
        assert_eq!(with_thousands(200_000), "200,000");
        assert_eq!(with_thousands(204_800), "204,800");
        assert_eq!(with_thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn no_quota_snapshots_returns_none_for_any_provider() {
        let nq = NoQuotaSnapshots;
        assert!(nq.quota_for("minimax").is_none());
        assert!(nq.quota_for("claude").is_none());
        assert!(nq.quota_for("unknown").is_none());
    }

    #[test]
    fn format_context_cell_wide_with_window() {
        assert_eq!(
            format_context_cell(12_304, Some(200_000), false),
            "ctx 12,304/200,000"
        );
    }

    #[test]
    fn format_context_cell_wide_without_window() {
        assert_eq!(format_context_cell(15_000, None, false), "ctx 15,000");
    }

    #[test]
    fn format_quota_cell_wide_with_quota() {
        let row = QuotaRow {
            used: 247,
            limit: 4500,
            window_label: "5h",
            status: QuotaBucket::Warn,
        };
        assert_eq!(format_quota_cell(Some(row), false), "quota 247/4500 (5h)");
    }

    #[test]
    fn format_quota_cell_wide_none_renders_dash() {
        assert_eq!(format_quota_cell(None, false), "-");
    }
}
