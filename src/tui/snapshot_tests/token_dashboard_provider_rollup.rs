use super::*;
use crate::budget::quota_snapshot::{QuotaBucket, QuotaRow};
use crate::budget::test_support::FakeProviderQuotaSnapshots;
use crate::session::types::SessionStatus;
use crate::tui::theme::Theme;
use crate::tui::token_dashboard::{draw_token_dashboard, draw_token_dashboard_with_quota};
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn token_dashboard_rollup_three_providers_120x40() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    let mut s_claude_1 =
        make_session_with_agent(SessionStatus::Running, Some(1), "claude", 0.10, 0.35);
    s_claude_1.id = uuid::Uuid::from_u128(1);
    s_claude_1.token_usage.input_tokens = 8_000;
    s_claude_1.token_usage.cache_read_tokens = 4_000;

    let mut s_claude_2 =
        make_session_with_agent(SessionStatus::Running, Some(2), "claude", 0.20, 0.40);
    s_claude_2.id = uuid::Uuid::from_u128(2);
    s_claude_2.token_usage.input_tokens = 0;
    s_claude_2.token_usage.cache_read_tokens = 0;

    let mut s_minimax =
        make_session_with_agent(SessionStatus::Running, Some(3), "minimax", 0.05, 0.80);
    s_minimax.id = uuid::Uuid::from_u128(3);
    s_minimax.model = "MiniMax-M1".to_string();
    s_minimax.token_usage.input_tokens = 4_200;
    s_minimax.token_usage.cache_read_tokens = 0;

    let mut s_ollama =
        make_session_with_agent(SessionStatus::Running, Some(4), "ollama", 0.00, 0.10);
    s_ollama.id = uuid::Uuid::from_u128(4);
    s_ollama.model = "llama3.2".to_string();
    s_ollama.token_usage.input_tokens = 1_000;
    s_ollama.token_usage.cache_read_tokens = 0;

    let quota = FakeProviderQuotaSnapshots::new().with(
        "minimax",
        QuotaRow {
            used: 247,
            limit: 4500,
            window_label: "5h",
            status: QuotaBucket::Warn,
        },
    );

    terminal
        .draw(|f| {
            draw_token_dashboard_with_quota(
                f,
                &[&s_claude_1, &s_claude_2, &s_minimax, &s_ollama],
                0.35,
                &quota,
                f.area(),
                &theme,
            );
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_rollup_narrow_80x24() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();

    let mut s_claude =
        make_session_with_agent(SessionStatus::Running, Some(1), "claude", 0.42, 0.35);
    s_claude.id = uuid::Uuid::from_u128(1);
    s_claude.token_usage.input_tokens = 12_304;

    let mut s_minimax =
        make_session_with_agent(SessionStatus::Running, Some(2), "minimax", 0.00, 0.80);
    s_minimax.id = uuid::Uuid::from_u128(2);
    s_minimax.model = "MiniMax-M1".to_string();
    s_minimax.token_usage.input_tokens = 4_200;

    let quota = FakeProviderQuotaSnapshots::new().with(
        "minimax",
        QuotaRow {
            used: 247,
            limit: 4500,
            window_label: "5h",
            status: QuotaBucket::Warn,
        },
    );

    terminal
        .draw(|f| {
            draw_token_dashboard_with_quota(
                f,
                &[&s_claude, &s_minimax],
                0.42,
                &quota,
                f.area(),
                &theme,
            );
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_rollup_zero_sessions() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            draw_token_dashboard(f, &[], 0.0, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_rollup_nan_cost_no_panic() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    let mut s = make_session_with_agent(SessionStatus::Running, Some(1), "claude", f64::NAN, 0.0);
    s.id = uuid::Uuid::from_u128(1);
    s.context_pct = f64::INFINITY;
    s.token_usage.input_tokens = 5_000;

    terminal
        .draw(|f| {
            draw_token_dashboard(f, &[&s], 0.0, f.area(), &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        !output.contains("NaN") && !output.contains("inf"),
        "rendered output must not contain NaN/inf literals"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_rollup_single_session() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    let mut s = make_session_with_agent(SessionStatus::Running, Some(1), "claude", 0.12, 0.30);
    s.id = uuid::Uuid::from_u128(1);
    s.token_usage.input_tokens = 6_000;
    s.token_usage.cache_read_tokens = 2_000;

    terminal
        .draw(|f| {
            draw_token_dashboard(f, &[&s], 0.12, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_rollup_unknown_agent_id() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    let mut s = make_session(SessionStatus::Running, Some(1));
    s.id = uuid::Uuid::from_u128(1);
    s.agent_id = None;
    s.cost_usd = 0.07;
    s.token_usage.input_tokens = 3_000;

    terminal
        .draw(|f| {
            draw_token_dashboard(f, &[&s], 0.07, f.area(), &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("unknown"),
        "rendered output must contain 'unknown' provider label.\n--- output ---\n{}\n--- end ---",
        output
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_rollup_quota_at_100pct() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    let mut s = make_session_with_agent(SessionStatus::Running, Some(1), "minimax", 0.00, 0.95);
    s.id = uuid::Uuid::from_u128(1);
    s.model = "MiniMax-M1".to_string();
    s.token_usage.input_tokens = 4_500;

    let quota = FakeProviderQuotaSnapshots::new().with(
        "minimax",
        QuotaRow {
            used: 4500,
            limit: 4500,
            window_label: "5h",
            status: QuotaBucket::Refused,
        },
    );

    terminal
        .draw(|f| {
            draw_token_dashboard_with_quota(f, &[&s], 0.0, &quota, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
