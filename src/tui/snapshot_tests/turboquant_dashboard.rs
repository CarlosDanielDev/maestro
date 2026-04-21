use super::*;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::session::types::{SessionStatus, TokenUsage};
use crate::tui::theme::Theme;
use crate::tui::turboquant_dashboard::draw_turboquant_dashboard;
use insta::assert_snapshot;

fn enabled_flags() -> FeatureFlags {
    let mut flags = FeatureFlags::default();
    flags.set_enabled(Flag::TurboQuant, true);
    flags
}

#[test]
fn turboquant_dashboard_projections_only_header_shows_estimated() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let flags = enabled_flags();

    let mut s1 = make_session(SessionStatus::Running, Some(1));
    s1.id = uuid::Uuid::from_u128(1);
    s1.issue_title = Some("Fix login flow".to_string());
    s1.token_usage = TokenUsage {
        input_tokens: 5_000,
        output_tokens: 500,
        ..Default::default()
    };
    s1.cost_usd = 0.02;

    let mut s2 = make_session(SessionStatus::Running, Some(2));
    s2.id = uuid::Uuid::from_u128(2);
    s2.issue_title = Some("Add cache layer".to_string());
    s2.token_usage = TokenUsage {
        input_tokens: 8_000,
        output_tokens: 400,
        ..Default::default()
    };
    s2.cost_usd = 0.03;

    terminal
        .draw(|f| {
            draw_turboquant_dashboard(f, &[&s1, &s2], &flags, 4, f.area(), &theme);
        })
        .unwrap();

    let out = format!("{}", terminal.backend());
    assert!(
        out.contains("Estimated Savings"),
        "projections-only header should read 'Estimated Savings'; got:\n{}",
        out
    );
    assert!(
        !out.contains("Actual Savings"),
        "no 'Actual Savings' text when no handoff data exists"
    );
    assert!(
        out.contains("proj."),
        "each row should be labeled 'proj.' when projection-only"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn turboquant_dashboard_mixed_actual_and_projections_header_shows_actual() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let flags = enabled_flags();

    let mut s_actual = make_session(SessionStatus::Completed, Some(10));
    s_actual.id = uuid::Uuid::from_u128(10);
    s_actual.issue_title = Some("Compress handoff".to_string());
    s_actual.token_usage = TokenUsage {
        input_tokens: 10_000,
        output_tokens: 500,
        ..Default::default()
    };
    s_actual.cost_usd = 0.05;
    s_actual.tq_handoff_original_tokens = Some(10_000);
    s_actual.tq_handoff_compressed_tokens = Some(2_500);

    let mut s_proj = make_session(SessionStatus::Running, Some(11));
    s_proj.id = uuid::Uuid::from_u128(11);
    s_proj.issue_title = Some("Pending work".to_string());
    s_proj.token_usage = TokenUsage {
        input_tokens: 3_000,
        output_tokens: 200,
        ..Default::default()
    };
    s_proj.cost_usd = 0.015;

    terminal
        .draw(|f| {
            draw_turboquant_dashboard(f, &[&s_actual, &s_proj], &flags, 4, f.area(), &theme);
        })
        .unwrap();

    let out = format!("{}", terminal.backend());
    assert!(
        out.contains("Actual Savings"),
        "mixed dashboard header should read 'Actual Savings'; got:\n{}",
        out
    );
    assert!(
        out.contains("ACTUAL"),
        "actual row should be marked 'ACTUAL'"
    );
    assert!(
        out.contains("proj."),
        "projection row should still appear with 'proj.' label"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn turboquant_dashboard_empty_sessions_renders_placeholder() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let flags = enabled_flags();

    terminal
        .draw(|f| {
            draw_turboquant_dashboard(f, &[], &flags, 4, f.area(), &theme);
        })
        .unwrap();

    let out = format!("{}", terminal.backend());
    assert!(out.contains("Estimated Savings"));
    assert!(out.contains("No session token data yet"));
    assert_snapshot!(terminal.backend());
}
