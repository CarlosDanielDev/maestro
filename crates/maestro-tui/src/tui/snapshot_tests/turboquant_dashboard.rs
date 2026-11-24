//! #1289: the TurboQuant panel of Dashboard › Tokens (the screen is gone).
//! Measured and estimated savings are separate rows; the estimate says it
//! is a ceiling, or "would save" while TurboQuant is off.

use super::*;
use crate::session::types::{SessionStatus, TokenUsage};
use crate::tui::theme::Theme;
use crate::tui::turboquant_dashboard::{classify_savings, draw_turboquant_panel};
use crate::turboquant::adapter::TurboQuantAdapter;
use insta::assert_snapshot;

fn session(issue: u64, input: u64, cost: f64) -> crate::session::types::Session {
    let mut s = make_session(SessionStatus::Running, Some(issue));
    s.id = uuid::Uuid::from_u128(u128::from(issue));
    s.token_usage = TokenUsage {
        input_tokens: input,
        output_tokens: 500,
        ..Default::default()
    };
    s.cost_usd = cost;
    s
}

fn panel(
    sessions: &[&crate::session::types::Session],
    enabled: bool,
) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal = test_terminal();
    let items = classify_savings(sessions, &TurboQuantAdapter::new(4));
    terminal
        .draw(|f| draw_turboquant_panel(f, &items, (enabled, 4), f.area(), &Theme::dark()))
        .unwrap();
    terminal
}

#[test]
fn turboquant_dashboard_projections_only_header_shows_estimated() {
    let (s1, s2) = (session(1, 5_000, 0.02), session(2, 8_000, 0.03));
    let terminal = panel(&[&s1, &s2], true);
    let out = format!("{}", terminal.backend());
    assert!(out.contains("TurboQuant · on · 4-bit"), "{out}");
    assert!(
        out.contains("~ estimate") && out.contains("a ceiling"),
        "{out}"
    );
    assert!(
        out.contains("✓ measured"),
        "the measured row is always shown: {out}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn turboquant_dashboard_mixed_actual_and_projections_header_shows_actual() {
    let mut actual = session(10, 10_000, 0.05);
    actual.status = SessionStatus::Completed;
    actual.tq_handoff_original_tokens = Some(10_000);
    actual.tq_handoff_compressed_tokens = Some(2_500);
    let proj = session(11, 3_000, 0.015);
    let terminal = panel(&[&actual, &proj], true);
    let out = format!("{}", terminal.backend());
    let measured = out
        .lines()
        .find(|l| l.contains("✓ measured"))
        .unwrap_or_default();
    assert!(
        measured.contains("7.5k"),
        "measured counts the handoff only: {out}"
    );
    assert!(out.contains("#10") && out.contains("#11"), "{out}");
    assert_snapshot!(terminal.backend());
}

#[test]
fn turboquant_dashboard_empty_sessions_renders_placeholder() {
    let terminal = panel(&[], true);
    let out = format!("{}", terminal.backend());
    assert!(out.contains("No token data yet"), "{out}");
    assert_snapshot!(terminal.backend());
}

#[test]
fn turboquant_panel_off_reads_would_save() {
    let s1 = session(1, 5_000, 0.02);
    let out = format!("{}", panel(&[&s1], false).backend());
    assert!(out.contains("TurboQuant · off"), "{out}");
    assert!(
        out.contains("would save") && !out.contains("a ceiling"),
        "{out}"
    );
}
