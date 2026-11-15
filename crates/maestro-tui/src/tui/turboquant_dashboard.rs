//! TurboQuant savings dashboard.
//!
//! Shows **honest** projections of token/cost savings based on actual
//! `TokenUsage` per session, and **real** savings when fork-handoff
//! compression (#343) has populated per-session handoff metrics.
//!
//! When at least one session has real handoff data, the header reads
//! "Actual Savings" and aggregate totals combine real + projected rows.
//! Otherwise the header reads "Estimated Savings (projection)" and every
//! row is a theoretical projection.

use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::session::types::Session;
use crate::tui::theme::Theme;
use crate::turboquant::adapter::{SavingsKind, SessionSavings, TurboQuantAdapter, session_savings};
use crate::util::formatting::format_tokens;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{BorderType, Paragraph},
};

/// Aggregate rollup of per-session savings shown on the dashboard.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AggregateSavings {
    pub total_saved_tokens: u64,
    pub total_saved_usd: f64,
    pub actual_count: usize,
    pub projection_count: usize,
}

/// Draw the TurboQuant savings dashboard. `bit_width` should come from
/// `config.turboquant.bit_width` so projections reflect the user's setting.
pub fn draw_turboquant_dashboard(
    f: &mut Frame,
    sessions: &[&Session],
    flags: &FeatureFlags,
    bit_width: u8,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // header + aggregate
            Constraint::Min(5),    // per-session breakdown
        ])
        .split(area);

    let adapter = TurboQuantAdapter::new(bit_width);
    let items = classify_savings(sessions, &adapter);
    let aggregate = aggregate_savings(&items);

    draw_summary(f, flags, aggregate, chunks[0], theme);
    draw_breakdown(f, &items, aggregate.actual_count > 0, chunks[1], theme);
}

/// Pair each session that produced savings data with its `SessionSavings`.
/// Sessions with no measurable data are skipped; pair order matches
/// `sessions` order.
pub fn classify_savings<'a>(
    sessions: &[&'a Session],
    adapter: &TurboQuantAdapter,
) -> Vec<(&'a Session, SessionSavings)> {
    sessions
        .iter()
        .filter_map(|s| session_savings(s, adapter).map(|sv| (*s, sv)))
        .collect()
}

/// Sum per-session savings into an `AggregateSavings`.
pub fn aggregate_savings(items: &[(&Session, SessionSavings)]) -> AggregateSavings {
    let mut agg = AggregateSavings::default();
    for (_, sv) in items {
        agg.total_saved_tokens = agg.total_saved_tokens.saturating_add(sv.saved_tokens);
        agg.total_saved_usd += sv.saved_usd;
        match sv.kind {
            SavingsKind::Actual => agg.actual_count += 1,
            SavingsKind::Projection => agg.projection_count += 1,
        }
    }
    agg
}

fn draw_summary(
    f: &mut Frame,
    flags: &FeatureFlags,
    agg: AggregateSavings,
    area: Rect,
    theme: &Theme,
) {
    let any_actual = agg.actual_count > 0;
    let tq_enabled = flags.is_enabled(Flag::TurboQuant);
    let status_text = if tq_enabled { "ON" } else { "OFF" };
    let status_color = if tq_enabled {
        theme.accent_success
    } else {
        theme.text_muted
    };

    let (header_text, header_style, border_type) = if any_actual {
        (
            "Actual Savings",
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
            BorderType::Plain,
        )
    } else {
        (
            "Estimated Savings (projection)",
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::ITALIC),
            BorderType::Rounded,
        )
    };

    let counts_text = if any_actual {
        format!(
            " Sessions: {} actual, {} projected ",
            agg.actual_count, agg.projection_count
        )
    } else {
        format!(" Sessions projected: {} ", agg.projection_count)
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
            Span::styled(counts_text, Style::default().fg(theme.text_secondary)),
        ]),
        Line::from(vec![
            Span::styled(" Tokens saved: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format_tokens(agg.total_saved_tokens),
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  USD saved: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("${:.4}", agg.total_saved_usd),
                Style::default().fg(theme.accent_warning),
            ),
        ]),
    ];

    let block = theme
        .styled_block(header_text, false)
        .border_type(border_type)
        .border_style(header_style);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_breakdown(
    f: &mut Frame,
    items: &[(&Session, SessionSavings)],
    any_actual: bool,
    area: Rect,
    theme: &Theme,
) {
    let title = if any_actual {
        "Savings Breakdown"
    } else {
        "Savings Breakdown (projected)"
    };

    if items.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            " No session token data yet. Run sessions to gather projections.",
            Style::default().fg(theme.text_muted),
        )))
        .block(theme.styled_block(title, false));
        f.render_widget(empty, area);
        return;
    }

    let header = Line::from(vec![Span::styled(
        format!(
            " {:>8}  {:>18}  {:>6}  {:>8}  {:>10}  {:>6}",
            "ID", "Title", "Kind", "Saved", "USD", "Ratio"
        ),
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD),
    )]);

    let max_rows = area.height.saturating_sub(3) as usize;
    let mut lines = vec![header];

    for (s, sv) in items.iter().take(max_rows) {
        let label = session_row_label(s);
        let title_text = session_row_title(s, 18);
        let (kind_style, row_modifier) = match sv.kind {
            SavingsKind::Actual => (
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
                Modifier::BOLD,
            ),
            SavingsKind::Projection => (
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
                Modifier::ITALIC,
            ),
        };

        let usd_text = if sv.saved_usd > 0.0 {
            format!("${:.4}", sv.saved_usd)
        } else {
            "—".to_string()
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:>8}", label),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled(
                format!("  {:>18}", title_text),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(format!("  {:>6}", sv.kind.label()), kind_style),
            Span::styled(
                format!("  {:>8}", format_tokens(sv.saved_tokens)),
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(row_modifier),
            ),
            Span::styled(
                format!("  {:>10}", usd_text),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(row_modifier),
            ),
            Span::styled(
                format!("  {:>5.1}x", sv.ratio),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(row_modifier),
            ),
        ]));
    }

    let block = theme.styled_block(title, false);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Session label for dashboard rows: issue number when present, else a short
/// UUID prefix.
pub fn session_row_label(s: &Session) -> String {
    match s.issue_number {
        Some(n) => format!("#{}", n),
        None => format!("S-{}", &s.id.to_string()[..8]),
    }
}

/// Session title truncated to `max_chars` for dashboard rows: issue title
/// when present, else a prompt prefix.
pub fn session_row_title(s: &Session, max_chars: usize) -> String {
    let fallback_len = s.prompt.len().min(max_chars);
    s.issue_title
        .as_deref()
        .unwrap_or(&s.prompt[..fallback_len])
        .chars()
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{SessionStatus, TokenUsage};
    use crate::turboquant::adapter::{SavingsKind, SessionSavings};

    fn adapter() -> TurboQuantAdapter {
        TurboQuantAdapter::new(4)
    }

    fn make_session(input_tokens: u64, cost_usd: f64, issue: Option<u64>) -> Session {
        let mut s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            issue,
            None,
        );
        s.status = SessionStatus::Running;
        s.token_usage = TokenUsage {
            input_tokens,
            ..Default::default()
        };
        s.cost_usd = cost_usd;
        s
    }

    fn make_session_with_handoff(
        input_tokens: u64,
        cost_usd: f64,
        original: u64,
        compressed: u64,
        issue: Option<u64>,
    ) -> Session {
        let mut s = make_session(input_tokens, cost_usd, issue);
        s.tq_handoff_original_tokens = Some(original);
        s.tq_handoff_compressed_tokens = Some(compressed);
        s
    }

    fn savings(kind: SavingsKind) -> SessionSavings {
        SessionSavings {
            kind,
            saved_tokens: 0,
            saved_usd: 0.0,
            ratio: 1.0,
        }
    }

    #[test]
    fn classify_pairs_sessions_with_savings() {
        let s1 = make_session(500, 0.001, Some(1));
        let s2 = make_session_with_handoff(500, 0.001, 1000, 250, Some(2));
        let items = classify_savings(&[&s1, &s2], &adapter());
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0.issue_number, Some(1));
        assert_eq!(items[1].0.issue_number, Some(2));
        assert_eq!(items[1].1.kind, SavingsKind::Actual);
    }

    #[test]
    fn classify_preserves_order_projections_only() {
        let s1 = make_session(1000, 0.0, Some(1));
        let s2 = make_session(500, 0.0, Some(2));
        let s3 = make_session(200, 0.0, Some(3));
        let items = classify_savings(&[&s1, &s2, &s3], &adapter());
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].1.saved_tokens, 750);
        assert_eq!(items[1].1.saved_tokens, 375);
        assert_eq!(items[2].1.saved_tokens, 150);
    }

    #[test]
    fn classify_skips_none_sessions() {
        let empty = make_session(0, 0.0, None);
        let real = make_session(500, 0.0, Some(1));
        let items = classify_savings(&[&empty, &real], &adapter());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0.issue_number, Some(1));
    }

    #[test]
    fn aggregate_totals_sums_across_kinds() {
        let s1 = make_session(0, 0.0, None);
        let s2 = make_session(0, 0.0, None);
        let items = vec![
            (
                &s1,
                SessionSavings {
                    kind: SavingsKind::Actual,
                    saved_tokens: 750,
                    saved_usd: 0.0015,
                    ratio: 4.0,
                },
            ),
            (
                &s2,
                SessionSavings {
                    kind: SavingsKind::Projection,
                    saved_tokens: 500,
                    saved_usd: 0.001,
                    ratio: 3.0,
                },
            ),
        ];
        let agg = aggregate_savings(&items);
        assert_eq!(agg.total_saved_tokens, 1250);
        assert!((agg.total_saved_usd - 0.0025).abs() < 1e-9);
    }

    #[test]
    fn aggregate_tracks_actual_and_projected_counts() {
        let s1 = make_session(0, 0.0, None);
        let s2 = make_session(0, 0.0, None);
        let s3 = make_session(0, 0.0, None);
        let items = vec![
            (&s1, savings(SavingsKind::Actual)),
            (&s2, savings(SavingsKind::Actual)),
            (&s3, savings(SavingsKind::Projection)),
        ];
        let agg = aggregate_savings(&items);
        assert_eq!(agg.actual_count, 2);
        assert_eq!(agg.projection_count, 1);
    }

    #[test]
    fn aggregate_empty_input() {
        let agg = aggregate_savings(&[]);
        assert_eq!(agg.total_saved_tokens, 0);
        assert!((agg.total_saved_usd - 0.0).abs() < f64::EPSILON);
        assert_eq!(agg.actual_count, 0);
        assert_eq!(agg.projection_count, 0);
    }

    #[test]
    fn classify_empty_sessions_returns_empty() {
        let items = classify_savings(&[], &adapter());
        assert!(items.is_empty());
    }
}
