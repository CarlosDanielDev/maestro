//! TurboQuant savings: the logic behind the Dashboard › Tokens tile and
//! panel (#1289). The full screen is gone; its ⌃K entry opens
//! Dashboard › Tokens (`screens::dashboard::usage::open_view`).
//!
//! Shows **honest** projections of token/cost savings based on actual
//! `TokenUsage` per session, and **real** savings when fork-handoff
//! compression (#343) has populated per-session handoff metrics. The two
//! are separate rows (`✓ measured`, `~ estimate`), never one total.

use crate::session::types::Session;
use crate::tui::ds::money::money;
use crate::tui::theme::Theme;
use crate::turboquant::adapter::{SavingsKind, SessionSavings, TurboQuantAdapter, session_savings};
use crate::util::formatting::format_tokens;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

/// Aggregate rollup of per-session savings shown on the dashboard.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AggregateSavings {
    pub total_saved_tokens: u64,
    pub total_saved_usd: f64,
    pub actual_count: usize,
    pub projection_count: usize,
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

/// The TurboQuant panel of Dashboard › Tokens (#1289; the screen is gone).
/// Measured (fork-handoff compression) and estimated savings are separate
/// rows; the estimate is a ceiling and says so. `tq` is (enabled, bits).
pub fn draw_turboquant_panel(
    f: &mut Frame,
    items: &[(&Session, SessionSavings)],
    tq: (bool, u8),
    area: Rect,
    theme: &Theme,
) {
    let (enabled, bits) = tq;
    let title = if enabled {
        format!("TurboQuant · on · {bits}-bit")
    } else {
        "TurboQuant · off".to_string()
    };
    let block = theme.styled_block(&title, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let muted = Style::default().fg(theme.text_muted);
    let text = Style::default().fg(theme.text_primary);
    if items.is_empty() {
        let lines = vec![
            Line::from(Span::styled(" No token data yet.", text)),
            Line::from(Span::styled(" It fills in as sessions run.", muted)),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }
    let (m_tok, m_usd) = savings_of(items, SavingsKind::Actual);
    let (e_tok, e_usd) = savings_of(items, SavingsKind::Projection);
    let estimate_word = if enabled { "a ceiling" } else { "would save" };
    let row = |glyph: &str, label: &str, tok: u64, usd: f64, note: &str| {
        Line::from(vec![
            Span::styled(format!(" {glyph} {label:<9}"), text),
            Span::styled(format!("{:>7}", format_tokens(tok)), text),
            Span::styled(format!("{:>8}", money(Some(usd))), text),
            Span::styled(format!("  {note}"), muted),
        ])
    };
    let mut lines = vec![
        row("✓", "measured", m_tok, m_usd, "fork handoffs"),
        row("~", "estimate", e_tok, e_usd, estimate_word),
        Line::from(Span::styled(
            format!(" est. = input − input ÷ {bits}, one fixed ratio"),
            muted,
        )),
    ];
    for (s, sv) in items {
        let priced = crate::tui::screens::dashboard::session_cost(s).is_some();
        let glyph = match sv.kind {
            SavingsKind::Actual => "✓",
            SavingsKind::Projection => "~",
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<7}", session_row_label(s)),
                Style::default().fg(theme.accent_identifier),
            ),
            Span::styled(format!("{glyph} "), text),
            Span::styled(format!("{:>7}", format_tokens(sv.saved_tokens)), text),
            // Priced at the session's own rate: unknown when it is unpriced.
            Span::styled(
                format!("{:>8}", money(priced.then_some(sv.saved_usd))),
                text,
            ),
            Span::styled(format!("{:>6.1}x", sv.ratio), muted),
        ]));
    }
    lines.push(Line::from(Span::styled(
        format!(
            " on/off: {}",
            crate::tui::navigation::registry::TURBOQUANT_TOGGLE
        ),
        muted,
    )));
    let lines: Vec<Line> = lines
        .into_iter()
        .map(|l| crate::tui::ds::fit::ellipsize_line(l, inner.width as usize))
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Tokens and dollars saved by the rows of one kind; measured and
/// estimated savings are never added together.
pub fn savings_of(items: &[(&Session, SessionSavings)], kind: SavingsKind) -> (u64, f64) {
    items
        .iter()
        .filter(|(_, sv)| sv.kind == kind)
        .fold((0, 0.0), |(t, u), (_, sv)| {
            (t.saturating_add(sv.saved_tokens), u + sv.saved_usd)
        })
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
