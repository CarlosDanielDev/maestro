use super::*;
use crate::tui::app::*;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
}

#[test]
fn tui_mode_completion_summary_variant_exists() {
    let mode = TuiMode::CompletionSummary;
    assert!(matches!(mode, TuiMode::CompletionSummary));
}

#[test]
fn completion_session_line_fields_are_accessible() {
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#42".to_string(),
        status: crate::session::types::SessionStatus::Completed,
        cost_usd: 1.23,
        elapsed: "1m 05s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![],
        worktree_path: None,
        issue_number: Some(42),
        model: "opus".to_string(),
    };
    assert_eq!(line.label, "#42");
    assert_eq!(line.status, crate::session::types::SessionStatus::Completed);
    assert!((line.cost_usd - 1.23).abs() < f64::EPSILON);
    assert_eq!(line.elapsed, "1m 05s");
}

#[test]
fn completion_summary_data_fields_are_accessible() {
    let data = CompletionSummaryData {
        sessions: vec![],
        total_cost_usd: 0.0,
        session_count: 0,
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(data.sessions.is_empty());
    assert_eq!(data.session_count, 0);
    assert!(data.total_cost_usd.abs() < f64::EPSILON);
}

#[test]
fn build_completion_summary_returns_empty_when_no_sessions() {
    let app = make_app();
    let summary = app.build_completion_summary();
    assert_eq!(summary.session_count, 0);
    assert!(summary.sessions.is_empty());
    assert!(summary.total_cost_usd.abs() < f64::EPSILON);
}

#[test]
fn build_completion_summary_label_uses_issue_number_when_present() {
    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "do something".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(99),
        None,
    );
    app.pool.enqueue(session);
    let summary = app.build_completion_summary();
    assert_eq!(summary.session_count, 1);
    assert!(
        summary.sessions[0].label.contains("#99"),
        "label must include issue number"
    );
}

#[test]
fn build_completion_summary_label_uses_short_id_when_no_issue() {
    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "do something".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    let short_id = session.id.to_string()[..8].to_string();
    app.pool.enqueue(session);
    let summary = app.build_completion_summary();
    assert_eq!(summary.session_count, 1);
    assert!(
        summary.sessions[0].label.contains(&short_id),
        "label must include short UUID when no issue"
    );
}

#[test]
fn build_completion_summary_aggregates_cost() {
    let mut app = make_app();
    let mut s1 = crate::session::types::Session::new(
        "task 1".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1),
        None,
    );
    s1.cost_usd = 1.50;
    let mut s2 = crate::session::types::Session::new(
        "task 2".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(2),
        None,
    );
    s2.cost_usd = 2.75;
    app.pool.enqueue(s1);
    app.pool.enqueue(s2);
    let summary = app.build_completion_summary();
    assert!((summary.total_cost_usd - 4.25).abs() < 0.001);
    assert_eq!(summary.session_count, 2);
}

#[test]
fn transition_to_dashboard_sets_tui_mode_to_dashboard() {
    let mut app = make_app();
    app.tui_mode = TuiMode::CompletionSummary;
    app.transition_to_dashboard();
    assert!(matches!(app.tui_mode, TuiMode::Dashboard));
}

#[test]
fn transition_to_dashboard_clears_completion_summary() {
    let mut app = make_app();
    app.completion_summary = Some(CompletionSummaryData {
        sessions: vec![],
        total_cost_usd: 0.0,
        session_count: 0,
        suggestions: vec![],
        selected_suggestion: 0,
    });
    app.transition_to_dashboard();
    assert!(app.completion_summary.is_none());
}

#[test]
fn transition_to_dashboard_preserves_orthogonal_state() {
    let mut app = make_app();
    app.total_cost = 9.99;
    app.running = true;
    app.transition_to_dashboard();
    assert!(app.running);
    assert!((app.total_cost - 9.99).abs() < f64::EPSILON);
}

#[test]
fn transition_to_dashboard_creates_home_screen_when_missing() {
    let mut app = make_app();
    assert!(app.screen_state.home_screen.is_none());
    app.transition_to_dashboard();
    assert!(app.screen_state.home_screen.is_some());
}

#[test]
fn transition_to_dashboard_queues_suggestion_refresh() {
    let mut app = make_app();
    app.transition_to_dashboard();
    assert!(
        app.pending_commands
            .iter()
            .any(|c| matches!(c, TuiCommand::FetchSuggestionData)),
        "must queue FetchSuggestionData"
    );
}

// --- Issue #86: suggestion refresh after session completion ---

#[test]
fn transition_to_dashboard_sets_loading_suggestions_flag() {
    let mut app = make_app();
    app.transition_to_dashboard();
    assert!(
        app.screen_state
            .home_screen
            .as_ref()
            .map(|s| s.loading_suggestions)
            .unwrap_or(false),
        "transition_to_dashboard must set loading_suggestions = true"
    );
}

#[test]
fn suggestion_data_event_clears_loading_flag_on_home_screen() {
    let mut app = make_app();
    app.transition_to_dashboard();
    if let Some(ref mut screen) = app.screen_state.home_screen {
        screen.loading_suggestions = true;
    }
    app.handle_data_event(TuiDataEvent::SuggestionData(Ok(SuggestionDataPayload {
        ready_issue_count: 0,
        failed_issue_count: 0,
        milestones: vec![],
        open_issue_count: 0,
        closed_issue_count: 0,
    })));
    assert!(
        !app.screen_state
            .home_screen
            .as_ref()
            .unwrap()
            .loading_suggestions,
        "SuggestionData event must clear loading_suggestions"
    );
}
