use super::helpers::{build_gate_fix_prompt, session_label};
use super::*;
use crate::flags::Flag;
use crate::session::types::GateResultEntry;
use std::time::Duration;

fn make_app() -> App {
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
fn app_once_mode_defaults_to_false() {
    let app = make_app();
    assert!(!app.once_mode, "once_mode must default to false");
}

#[test]
fn app_completion_summary_defaults_to_none() {
    let app = make_app();
    assert!(
        app.completion_summary.is_none(),
        "completion_summary must default to None"
    );
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
    );
    s1.cost_usd = 1.50;
    let mut s2 = crate::session::types::Session::new(
        "task 2".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(2),
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
    assert!(app.home_screen.is_none());
    app.transition_to_dashboard();
    assert!(app.home_screen.is_some());
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
        app.home_screen
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
    if let Some(ref mut screen) = app.home_screen {
        screen.loading_suggestions = true;
    }
    app.handle_data_event(TuiDataEvent::SuggestionData(SuggestionDataPayload {
        ready_issue_count: 0,
        failed_issue_count: 0,
        milestones: vec![],
        open_issue_count: 0,
        closed_issue_count: 0,
    }));
    assert!(
        !app.home_screen.as_ref().unwrap().loading_suggestions,
        "SuggestionData event must clear loading_suggestions"
    );
}

// --- Issue #84: post-session activity log with cost summary ---

#[test]
fn completion_session_line_pr_link_defaults_to_empty() {
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#1".to_string(),
        status: crate::session::types::SessionStatus::Completed,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![],
        issue_number: Some(1),
        model: "opus".to_string(),
    };
    assert!(line.pr_link.is_empty());
}

#[test]
fn completion_session_line_holds_pr_link_value() {
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#42".to_string(),
        status: crate::session::types::SessionStatus::Completed,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: "https://github.com/org/repo/pull/42".into(),
        error_summary: String::new(),
        gate_failures: vec![],
        issue_number: Some(42),
        model: "opus".to_string(),
    };
    assert_eq!(line.pr_link, "https://github.com/org/repo/pull/42");
}

#[test]
fn completion_session_line_holds_error_summary_value() {
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#7".to_string(),
        status: crate::session::types::SessionStatus::Errored,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: String::new(),
        error_summary: "Error: process exited with code 1".into(),
        gate_failures: vec![],
        issue_number: Some(7),
        model: "opus".to_string(),
    };
    assert_eq!(line.error_summary, "Error: process exited with code 1");
}

#[test]
fn build_completion_summary_sets_pr_link_when_pending_check_matches() {
    use crate::provider::github::ci::PendingPrCheck;

    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(10),
    );
    session.status = crate::session::types::SessionStatus::Completed;
    app.pool.enqueue(session);
    app.ci_poller.add_check(PendingPrCheck {
        pr_number: 42,
        issue_number: 10,
        branch: "feat/issue-10".into(),
        created_at: std::time::Instant::now(),
        check_count: 0,
        fix_attempt: 0,
        awaiting_fix_ci: false,
    });

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].pr_link.contains("42"),
        "pr_link must reference PR number"
    );
}

#[test]
fn build_completion_summary_pr_link_empty_when_no_matching_check() {
    use crate::provider::github::ci::PendingPrCheck;

    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(99),
    );
    app.pool.enqueue(session);
    app.ci_poller.add_check(PendingPrCheck {
        pr_number: 5,
        issue_number: 5,
        branch: "feat/issue-5".into(),
        created_at: std::time::Instant::now(),
        check_count: 0,
        fix_attempt: 0,
        awaiting_fix_ci: false,
    });

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].pr_link.is_empty(),
        "pr_link must be empty when no PendingPrCheck matches"
    );
}

#[test]
fn build_completion_summary_pr_link_empty_when_no_issue_number() {
    use crate::provider::github::ci::PendingPrCheck;

    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
    );
    app.pool.enqueue(session);
    app.ci_poller.add_check(PendingPrCheck {
        pr_number: 1,
        issue_number: 1,
        branch: "feat/issue-1".into(),
        created_at: std::time::Instant::now(),
        check_count: 0,
        fix_attempt: 0,
        awaiting_fix_ci: false,
    });

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].pr_link.is_empty(),
        "pr_link must be empty for sessions without issue_number"
    );
}

#[test]
fn build_completion_summary_error_summary_for_errored_session() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(5),
    );
    session.status = crate::session::types::SessionStatus::Errored;
    session.log_activity("Process started".into());
    session.log_activity("Error: process exited with code 1".into());
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(
        !summary.sessions[0].error_summary.is_empty(),
        "error_summary must be set for Errored sessions with activity"
    );
}

#[test]
fn build_completion_summary_error_summary_empty_for_completed() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(6),
    );
    session.status = crate::session::types::SessionStatus::Completed;
    session.log_activity("Some activity".into());
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].error_summary.is_empty(),
        "error_summary must be empty for Completed sessions"
    );
}

#[test]
fn build_completion_summary_error_summary_empty_when_no_activity() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(7),
    );
    session.status = crate::session::types::SessionStatus::Errored;
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].error_summary.is_empty(),
        "error_summary must be empty when activity_log is empty"
    );
}

#[test]
fn build_completion_summary_error_summary_truncates_long_messages() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(8),
    );
    session.status = crate::session::types::SessionStatus::Errored;
    session.log_activity(format!("Error: {}", "x".repeat(200)));
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    let err = &summary.sessions[0].error_summary;
    assert!(
        err.len() <= 83,
        "error_summary must be truncated, got {} chars",
        err.len()
    );
    assert!(err.ends_with("..."), "truncated summary must end with ...");
}

#[test]
fn build_completion_summary_pr_link_from_ci_fix_context() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "fix ci".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(15),
    );
    session.status = crate::session::types::SessionStatus::Completed;
    session.ci_fix_context = Some(crate::session::types::CiFixContext {
        pr_number: 77,
        issue_number: 15,
        branch: "feat/fix-ci".into(),
        attempt: 1,
    });
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].pr_link.contains("77"),
        "pr_link must reference ci_fix_context PR number"
    );
}

// --- Issue #104: [f] Fix action in completion overlay for failed gates ---

#[test]
fn gate_failure_info_fields_are_accessible() {
    let info = GateFailureInfo {
        gate: "tests".to_string(),
        message: "3 tests failed".to_string(),
    };
    assert_eq!(info.gate, "tests");
    assert_eq!(info.message, "3 tests failed");
}

#[test]
fn gate_failure_info_can_be_cloned() {
    let info = GateFailureInfo {
        gate: "clippy".to_string(),
        message: "2 warnings".to_string(),
    };
    let cloned = info.clone();
    assert_eq!(cloned.gate, info.gate);
    assert_eq!(cloned.message, info.message);
}

#[test]
fn completion_session_line_gate_failures_defaults_to_empty() {
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#42".to_string(),
        status: crate::session::types::SessionStatus::NeedsReview,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![],
        issue_number: Some(42),
        model: "opus".to_string(),
    };
    assert!(line.gate_failures.is_empty());
}

#[test]
fn completion_session_line_holds_gate_failures() {
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#7".to_string(),
        status: crate::session::types::SessionStatus::NeedsReview,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![GateFailureInfo {
            gate: "tests".to_string(),
            message: "cargo test failed".to_string(),
        }],
        issue_number: Some(7),
        model: "opus".to_string(),
    };
    assert_eq!(line.gate_failures.len(), 1);
    assert_eq!(line.gate_failures[0].gate, "tests");
}

#[test]
fn has_needs_review_returns_false_when_no_sessions() {
    let data = CompletionSummaryData {
        sessions: vec![],
        total_cost_usd: 0.0,
        session_count: 0,
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(!data.has_needs_review());
}

#[test]
fn has_needs_review_returns_false_when_all_completed() {
    let data = CompletionSummaryData {
        sessions: vec![CompletionSessionLine {
            session_id: uuid::Uuid::nil(),
            label: "#1".to_string(),
            status: crate::session::types::SessionStatus::Completed,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![],
            issue_number: Some(1),
            model: "opus".to_string(),
        }],
        total_cost_usd: 0.0,
        session_count: 1,
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(!data.has_needs_review());
}

#[test]
fn has_needs_review_returns_true_when_one_session_needs_review() {
    let data = CompletionSummaryData {
        sessions: vec![CompletionSessionLine {
            session_id: uuid::Uuid::nil(),
            label: "#2".to_string(),
            status: crate::session::types::SessionStatus::NeedsReview,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![GateFailureInfo {
                gate: "tests".to_string(),
                message: "failed".to_string(),
            }],
            issue_number: Some(2),
            model: "opus".to_string(),
        }],
        total_cost_usd: 0.0,
        session_count: 1,
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(data.has_needs_review());
}

#[test]
fn has_needs_review_returns_true_when_mixed_statuses() {
    let data = CompletionSummaryData {
        sessions: vec![
            CompletionSessionLine {
                session_id: uuid::Uuid::nil(),
                label: "#1".to_string(),
                status: crate::session::types::SessionStatus::Completed,
                cost_usd: 0.0,
                elapsed: "0s".to_string(),
                pr_link: String::new(),
                error_summary: String::new(),
                gate_failures: vec![],
                issue_number: Some(1),
                model: "opus".to_string(),
            },
            CompletionSessionLine {
                session_id: uuid::Uuid::nil(),
                label: "#2".to_string(),
                status: crate::session::types::SessionStatus::NeedsReview,
                cost_usd: 0.0,
                elapsed: "0s".to_string(),
                pr_link: String::new(),
                error_summary: String::new(),
                gate_failures: vec![GateFailureInfo {
                    gate: "clippy".to_string(),
                    message: "lint error".to_string(),
                }],
                issue_number: Some(2),
                model: "opus".to_string(),
            },
        ],
        total_cost_usd: 0.0,
        session_count: 2,
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(data.has_needs_review());
}

#[test]
fn build_completion_summary_gate_failures_empty_for_completed_session() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(10),
    );
    session.status = crate::session::types::SessionStatus::Completed;
    session.gate_results = vec![GateResultEntry::pass("tests", "all passed")];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(
        summary.sessions[0].gate_failures.is_empty(),
        "completed session with passing gates must have empty gate_failures"
    );
}

#[test]
fn build_completion_summary_gate_failures_populated_for_needs_review() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(20),
    );
    session.status = crate::session::types::SessionStatus::NeedsReview;
    session.gate_results = vec![GateResultEntry::fail("tests", "3 tests failed")];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(summary.sessions[0].gate_failures.len(), 1);
    assert_eq!(summary.sessions[0].gate_failures[0].gate, "tests");
    assert!(
        summary.sessions[0].gate_failures[0]
            .message
            .contains("3 tests failed")
    );
}

#[test]
fn build_completion_summary_gate_failures_multiple_failed_gates() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(30),
    );
    session.status = crate::session::types::SessionStatus::NeedsReview;
    session.gate_results = vec![
        GateResultEntry::fail("tests", "cargo test failed"),
        GateResultEntry::fail("clippy", "2 warnings"),
    ];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(summary.sessions[0].gate_failures.len(), 2);
}

#[test]
fn build_completion_summary_gate_failures_skips_passing_gates() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(40),
    );
    session.status = crate::session::types::SessionStatus::NeedsReview;
    session.gate_results = vec![
        GateResultEntry::pass("fmt", "formatted"),
        GateResultEntry::fail("tests", "failed"),
    ];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(summary.sessions[0].gate_failures.len(), 1);
    assert_eq!(summary.sessions[0].gate_failures[0].gate, "tests");
}

#[test]
fn build_completion_summary_gate_failures_empty_when_needs_review_has_no_gate_results() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(50),
    );
    session.status = crate::session::types::SessionStatus::NeedsReview;
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(summary.sessions[0].gate_failures.is_empty());
}

#[test]
fn build_completion_summary_populates_issue_number() {
    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(77),
    );
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(summary.sessions[0].issue_number, Some(77));
}

#[test]
fn build_completion_summary_issue_number_is_none_when_session_has_none() {
    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
    );
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert!(summary.sessions[0].issue_number.is_none());
}

#[test]
fn build_completion_summary_populates_model() {
    let mut app = make_app();
    let session = crate::session::types::Session::new(
        "task".into(),
        "claude-opus-4".into(),
        "orchestrator".into(),
        Some(88),
    );
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(summary.sessions[0].model, "claude-opus-4");
}

#[test]
fn build_completion_summary_gate_failure_message_truncated() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(60),
    );
    session.status = crate::session::types::SessionStatus::NeedsReview;
    session.gate_results = vec![GateResultEntry::fail("tests", &"x".repeat(300))];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    let msg = &summary.sessions[0].gate_failures[0].message;
    assert!(
        msg.len() <= 104,
        "gate failure message must be truncated, got {} chars",
        msg.len()
    );
}

#[test]
fn build_gate_fix_prompt_includes_issue_number() {
    let prompt = build_gate_fix_prompt(42, "tests failed");
    assert!(prompt.contains("42"));
}

#[test]
fn build_gate_fix_prompt_includes_failure_details() {
    let details = "- [tests]: cargo test -- 3 failures";
    let prompt = build_gate_fix_prompt(10, details);
    assert!(prompt.contains(details));
}

#[test]
fn build_gate_fix_prompt_is_non_empty() {
    let prompt = build_gate_fix_prompt(99, "gate failed");
    assert!(!prompt.is_empty());
}

#[test]
fn spawn_gate_fix_session_queues_pending_launch() {
    let mut app = make_app();

    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#55".to_string(),
        status: crate::session::types::SessionStatus::NeedsReview,
        cost_usd: 1.0,
        elapsed: "30s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![GateFailureInfo {
            gate: "tests".to_string(),
            message: "cargo test failed".to_string(),
        }],
        issue_number: Some(55),
        model: "opus".to_string(),
    };

    app.spawn_gate_fix_session(&line);
    assert!(
        !app.pending_session_launches.is_empty(),
        "spawn_gate_fix_session must queue a session launch"
    );
}

#[test]
fn spawn_gate_fix_session_does_nothing_when_no_issue_number() {
    let mut app = make_app();
    let line = CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "abc123".to_string(),
        status: crate::session::types::SessionStatus::NeedsReview,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![GateFailureInfo {
            gate: "tests".to_string(),
            message: "failed".to_string(),
        }],
        issue_number: None,
        model: "opus".to_string(),
    };

    app.spawn_gate_fix_session(&line);
    assert!(
        app.pending_session_launches.is_empty(),
        "spawn_gate_fix_session must be a no-op when issue_number is None"
    );
}

// --- Issue #148: completion summary traps navigation ---

#[test]
fn completion_summary_dismissed_defaults_to_false() {
    let app = make_app();
    assert!(
        !app.completion_summary_dismissed,
        "completion_summary_dismissed must default to false"
    );
}

#[test]
fn transition_to_dashboard_sets_dismissed_flag() {
    let mut app = make_app();
    app.tui_mode = TuiMode::CompletionSummary;
    app.completion_summary = Some(CompletionSummaryData {
        sessions: vec![],
        total_cost_usd: 0.0,
        session_count: 0,
        suggestions: vec![],
        selected_suggestion: 0,
    });
    app.transition_to_dashboard();
    assert!(
        app.completion_summary_dismissed,
        "transition_to_dashboard must set completion_summary_dismissed = true"
    );
    assert!(
        app.completion_summary.is_none(),
        "completion_summary must be cleared"
    );
    assert!(
        matches!(app.tui_mode, TuiMode::Dashboard),
        "tui_mode must be Dashboard"
    );
}

#[test]
fn transition_to_dashboard_clears_issue_browser_screen() {
    let mut app = make_app();
    app.issue_browser_screen = Some(crate::tui::screens::IssueBrowserScreen::new(vec![]));
    app.transition_to_dashboard();
    assert!(
        app.issue_browser_screen.is_none(),
        "transition_to_dashboard must clear stale issue_browser_screen"
    );
}

#[tokio::test]
async fn add_session_resets_dismissed_flag() {
    let mut app = make_app();
    app.completion_summary_dismissed = true;
    let session = crate::session::types::Session::new(
        "test".to_string(),
        "opus".to_string(),
        "orchestrator".to_string(),
        None,
    );
    let _ = app.add_session(session).await;
    assert!(
        !app.completion_summary_dismissed,
        "add_session must reset completion_summary_dismissed to false"
    );
}

#[test]
fn dismissed_flag_prevents_summary_retrigger_scenario() {
    // Simulate: completion summary was shown, user dismissed it,
    // all sessions are still done → summary must NOT re-trigger.
    let mut app = make_app();
    // Set up a finished session
    let session = crate::session::types::Session::new(
        "done".to_string(),
        "opus".to_string(),
        "orchestrator".to_string(),
        None,
    );
    app.pool.enqueue(session);
    let promoted = app.pool.try_promote();
    for id in promoted {
        app.pool.on_session_completed(id);
    }
    assert!(app.pool.all_done(), "pool must be all_done");

    // Dismiss the summary
    app.completion_summary_dismissed = true;
    app.completion_summary = None;
    app.tui_mode = TuiMode::Overview;

    // The auto-transition condition should NOT fire:
    // all_done=true, continuous_mode=None, completion_summary=None,
    // but completion_summary_dismissed=true → blocked
    let should_trigger = app.pool.all_done()
        && app.continuous_mode.is_none()
        && app.completion_summary.is_none()
        && !app.completion_summary_dismissed;
    assert!(
        !should_trigger,
        "auto-transition must not fire when completion_summary_dismissed is true"
    );
}

// --- Issue #145: FeatureFlags wiring ---

fn make_app_with_flags(flags: crate::flags::store::FeatureFlags) -> App {
    let mut app = make_app();
    app.flags = flags;
    app
}

#[test]
fn app_flags_field_defaults_to_feature_flags_default() {
    let app = make_app();
    assert!(
        app.flags.is_enabled(Flag::ContinuousMode),
        "app.flags must default ContinuousMode to true"
    );
    assert!(
        app.flags.is_enabled(Flag::AutoFork),
        "app.flags must default AutoFork to true"
    );
    assert!(
        !app.flags.is_enabled(Flag::CiAutoFix),
        "app.flags must default CiAutoFix to false"
    );
}

#[test]
fn app_flags_can_be_replaced_after_construction() {
    let mut app = make_app();
    let custom = crate::flags::store::FeatureFlags::new(
        std::collections::HashMap::new(),
        vec!["ci_auto_fix".to_string()],
        vec![],
    );
    app.flags = custom;
    assert!(
        app.flags.is_enabled(Flag::CiAutoFix),
        "app.flags must reflect newly assigned FeatureFlags"
    );
}

#[test]
fn continuous_mode_not_set_when_flag_disabled() {
    let flags = crate::flags::store::FeatureFlags::new(
        std::collections::HashMap::new(),
        vec![],
        vec!["continuous_mode".to_string()],
    );
    let mut app = make_app_with_flags(flags);
    // Simulate gating logic from cmd_run
    let cli_continuous = true;
    if app.flags.is_enabled(Flag::ContinuousMode) && cli_continuous {
        app.continuous_mode = Some(ContinuousModeState::new());
    }
    assert!(
        app.continuous_mode.is_none(),
        "continuous_mode must remain None when Flag::ContinuousMode is disabled"
    );
}

#[test]
fn continuous_mode_set_when_flag_enabled() {
    let mut app = make_app();
    let cli_continuous = true;
    if app.flags.is_enabled(Flag::ContinuousMode) && cli_continuous {
        app.continuous_mode = Some(ContinuousModeState::new());
    }
    assert!(
        app.continuous_mode.is_some(),
        "continuous_mode must be Some when Flag::ContinuousMode is enabled"
    );
}

#[test]
fn check_context_overflow_skips_fork_when_auto_fork_flag_disabled() {
    let flags = crate::flags::store::FeatureFlags::new(
        std::collections::HashMap::new(),
        vec![],
        vec!["auto_fork".to_string()],
    );
    let mut app = make_app_with_flags(flags);
    app.fork_policy = Some(crate::session::fork::ForkPolicy::new(5));
    let dummy_id = uuid::Uuid::new_v4();
    app.check_context_overflow(dummy_id);
    assert!(
        app.pending_session_launches.is_empty(),
        "check_context_overflow must not fork when Flag::AutoFork is disabled"
    );
}

#[test]
fn poll_ci_status_skips_fix_when_ci_auto_fix_flag_disabled() {
    use crate::provider::github::ci::PendingPrCheck;
    let flags = crate::flags::store::FeatureFlags::new(
        std::collections::HashMap::new(),
        vec![],
        vec!["ci_auto_fix".to_string()],
    );
    let mut app = make_app_with_flags(flags);
    app.ci_poller.add_check(PendingPrCheck {
        pr_number: 99,
        issue_number: 42,
        branch: "feat/test".to_string(),
        fix_attempt: 0,
        check_count: 0,
        awaiting_fix_ci: false,
        created_at: Instant::now()
            .checked_sub(Duration::from_secs(120))
            .unwrap_or_else(Instant::now),
    });
    app.ci_poller.last_ci_poll = Instant::now()
        .checked_sub(Duration::from_secs(120))
        .unwrap_or_else(Instant::now);
    // poll_ci_status with Flag::CiAutoFix disabled — no fix sessions spawned.
    // The checker will fail (no gh in tests), but auto_fix_enabled is false so
    // CiPollAction::Abandon is chosen — no fix session enqueued.
    app.poll_ci_status();
    assert!(
        app.pending_session_launches.is_empty(),
        "poll_ci_status must not spawn fix session when Flag::CiAutoFix is disabled"
    );
}

// --- Issue #125: CI check details field ---

#[test]
fn app_ci_check_details_field_defaults_to_empty() {
    let app = make_app();
    assert!(app.ci_poller.ci_check_details.is_empty());
}

#[test]
fn ci_check_details_can_be_populated_and_read() {
    let mut app = make_app();
    let detail = crate::provider::github::ci::CheckRunDetail {
        name: "build".into(),
        status: crate::provider::github::ci::CheckStatus::Completed,
        conclusion: crate::provider::github::ci::CheckConclusion::Success,
        started_at: None,
        elapsed_secs: Some(42),
    };
    app.ci_poller.ci_check_details.insert(99, vec![detail]);
    assert_eq!(app.ci_poller.ci_check_details.len(), 1);
    assert_eq!(app.ci_poller.ci_check_details[&99][0].name, "build");
}

#[test]
fn ci_check_details_keyed_by_pr_number() {
    let mut app = make_app();
    let detail = crate::provider::github::ci::CheckRunDetail {
        name: "test".into(),
        status: crate::provider::github::ci::CheckStatus::InProgress,
        conclusion: crate::provider::github::ci::CheckConclusion::None,
        started_at: None,
        elapsed_secs: None,
    };
    app.ci_poller.ci_check_details.insert(55, vec![detail]);
    assert!(app.ci_poller.ci_check_details.contains_key(&55));
    assert!(!app.ci_poller.ci_check_details.contains_key(&10));
}

// --- Issue #67: QueueConfirmation screen state ---

#[test]
fn app_queue_confirmation_screen_defaults_to_none() {
    let app = make_app();
    assert!(app.queue_confirmation_screen.is_none());
}

// --- Issue #68: QueueExecutor state ---

#[test]
fn app_queue_executor_defaults_to_none() {
    let app = make_app();
    assert!(app.queue_executor.is_none());
}

#[test]
fn tui_mode_queue_execution_variant_exists() {
    let mode = TuiMode::QueueExecution;
    assert!(matches!(mode, TuiMode::QueueExecution));
}

// --- Issue #139: ConflictSuggestion and CompletionSummaryData suggestions ---

#[test]
fn conflict_suggestion_stores_all_fields() {
    let sg = ConflictSuggestion {
        pr_number: 42,
        issue_number: 10,
        branch: "feat/auth".to_string(),
        conflicting_files: vec!["src/a.rs".to_string()],
        message: "has merge conflicts".to_string(),
    };
    assert_eq!(sg.pr_number, 42);
    assert_eq!(sg.issue_number, 10);
    assert_eq!(sg.branch, "feat/auth");
    assert_eq!(sg.conflicting_files.len(), 1);
    assert!(!sg.message.is_empty());
}

#[test]
fn completion_summary_data_suggestions_defaults_to_empty() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(data.suggestions.is_empty());
}

#[test]
fn completion_summary_data_selected_suggestion_defaults_to_zero() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert_eq!(data.selected_suggestion, 0);
}

#[test]
fn completion_summary_data_holds_multiple_suggestions() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![
            ConflictSuggestion {
                pr_number: 1,
                issue_number: 1,
                branch: "a".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
            ConflictSuggestion {
                pr_number: 2,
                issue_number: 2,
                branch: "b".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
        ],
        selected_suggestion: 0,
    };
    assert_eq!(data.suggestions.len(), 2);
}

#[test]
fn selected_suggestion_can_be_advanced() {
    let mut data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![
            ConflictSuggestion {
                pr_number: 1,
                issue_number: 1,
                branch: "a".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
            ConflictSuggestion {
                pr_number: 2,
                issue_number: 2,
                branch: "b".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
        ],
        selected_suggestion: 0,
    };
    data.selected_suggestion = 1;
    assert_eq!(data.selected_suggestion, 1);
}

#[test]
fn has_conflict_suggestions_returns_false_when_empty() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(!data.has_conflict_suggestions());
}

#[test]
fn has_conflict_suggestions_returns_true_when_populated() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![ConflictSuggestion {
            pr_number: 1,
            issue_number: 1,
            branch: "a".to_string(),
            conflicting_files: vec![],
            message: "conflict".to_string(),
        }],
        selected_suggestion: 0,
    };
    assert!(data.has_conflict_suggestions());
}

#[test]
fn build_completion_summary_suggestions_default_to_empty() {
    let app = make_app();
    let summary = app.build_completion_summary();
    assert!(summary.suggestions.is_empty());
    assert_eq!(summary.selected_suggestion, 0);
}

// --- SessionSummaryState (#265) ---

#[test]
fn session_summary_toggle_expand_adds_id() {
    let mut state = crate::tui::app::types::SessionSummaryState::default();
    let id = uuid::Uuid::new_v4();
    state.toggle_expand(id);
    assert!(state.expanded.contains(&id));
}

#[test]
fn session_summary_toggle_expand_removes_id_on_second_call() {
    let mut state = crate::tui::app::types::SessionSummaryState::default();
    let id = uuid::Uuid::new_v4();
    state.toggle_expand(id);
    state.toggle_expand(id);
    assert!(!state.expanded.contains(&id));
}

#[test]
fn session_summary_toggle_expand_two_ids_independent() {
    let mut state = crate::tui::app::types::SessionSummaryState::default();
    let id_a = uuid::Uuid::new_v4();
    let id_b = uuid::Uuid::new_v4();
    state.toggle_expand(id_a);
    state.toggle_expand(id_b);
    state.toggle_expand(id_a);
    assert!(!state.expanded.contains(&id_a));
    assert!(state.expanded.contains(&id_b));
}

#[test]
fn session_summary_scroll_down_increments_offset() {
    let mut state = crate::tui::app::types::SessionSummaryState::default();
    assert_eq!(state.scroll_offset, 0);
    state.scroll_down();
    assert_eq!(state.scroll_offset, 1);
    state.scroll_down();
    assert_eq!(state.scroll_offset, 2);
}

#[test]
fn session_summary_scroll_up_decrements_offset() {
    let mut state = crate::tui::app::types::SessionSummaryState::default();
    state.scroll_down();
    state.scroll_down();
    state.scroll_up();
    assert_eq!(state.scroll_offset, 1);
}

#[test]
fn session_summary_scroll_up_does_not_underflow() {
    let mut state = crate::tui::app::types::SessionSummaryState::default();
    state.scroll_up();
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn tui_mode_session_summary_variant_exists() {
    let mode = crate::tui::app::types::TuiMode::SessionSummary;
    assert!(matches!(
        mode,
        crate::tui::app::types::TuiMode::SessionSummary
    ));
}

// -- Adapt pipeline data chaining integration tests --

mod adapt_chaining {
    use super::*;
    use crate::adapt::types::*;
    use crate::tui::screens::adapt::{AdaptScreen, AdaptStep};

    fn make_profile() -> ProjectProfile {
        ProjectProfile {
            name: "test".into(),
            root: std::path::PathBuf::from("/tmp"),
            language: ProjectLanguage::Rust,
            manifests: vec![],
            config_files: vec![],
            entry_points: vec![],
            source_stats: SourceStats {
                total_files: 10,
                total_lines: 500,
                by_extension: vec![],
            },
            test_infra: TestInfraInfo {
                has_tests: true,
                framework: None,
                test_directories: vec![],
                test_file_count: 0,
            },
            ci: CiInfo {
                provider: None,
                config_files: vec![],
            },
            git: GitInfo {
                is_git_repo: true,
                default_branch: Some("main".into()),
                remote_url: None,
                commit_count: 10,
                recent_contributors: vec![],
            },
            dependencies: DependencySummary::default(),
            directory_tree: String::new(),
            has_maestro_config: false,
            has_workflow_docs: false,
        }
    }

    fn make_report() -> AdaptReport {
        AdaptReport {
            summary: "Test".into(),
            modules: vec![],
            tech_debt_items: vec![],
        }
    }

    fn make_plan() -> AdaptPlan {
        AdaptPlan {
            milestones: vec![],
            maestro_toml_patch: None,
            workflow_guide: None,
        }
    }

    fn make_materialize_result() -> MaterializeResult {
        MaterializeResult {
            milestones_created: vec![],
            issues_created: vec![],
            issues_skipped: vec![],
            tech_debt_issue: None,
            dry_run: false,
        }
    }

    fn app_with_adapt_screen() -> App {
        let mut app = make_app();
        app.adapt_screen = Some(AdaptScreen::new());
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Scanning;
        app
    }

    #[test]
    fn scan_ok_chains_to_analyze() {
        let mut app = app_with_adapt_screen();
        app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Analyzing);
        assert!(screen.results.profile.is_some());
        assert_eq!(app.pending_commands.len(), 1);
        assert!(matches!(
            app.pending_commands[0],
            TuiCommand::RunAdaptAnalyze(_, _)
        ));
    }

    #[test]
    fn scan_ok_with_scan_only_completes() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().config.scan_only = true;

        app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Complete);
        assert!(app.pending_commands.is_empty());
    }

    #[test]
    fn scan_err_sets_failed() {
        let mut app = app_with_adapt_screen();
        app.handle_data_event(TuiDataEvent::AdaptScanResult(Err(anyhow::anyhow!(
            "scan failed"
        ))));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Failed);
        assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Scanning);
    }

    #[test]
    fn analyze_ok_chains_to_consolidate() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Analyzing;
        app.adapt_screen
            .as_mut()
            .unwrap()
            .set_scan_result(make_profile());

        app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Ok(make_report())));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Consolidating);
        assert!(screen.results.report.is_some());
        assert_eq!(app.pending_commands.len(), 1);
        assert!(matches!(
            app.pending_commands[0],
            TuiCommand::RunAdaptConsolidate(_, _, _)
        ));
    }

    #[test]
    fn analyze_ok_with_no_issues_completes() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Analyzing;
        app.adapt_screen.as_mut().unwrap().config.no_issues = true;

        app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Ok(make_report())));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Complete);
        assert!(app.pending_commands.is_empty());
    }

    #[test]
    fn analyze_err_sets_failed() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Analyzing;

        app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Err(anyhow::anyhow!(
            "analyze failed"
        ))));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Failed);
        assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Analyzing);
    }

    #[test]
    fn plan_ok_chains_to_materialize() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Planning;
        app.adapt_screen
            .as_mut()
            .unwrap()
            .set_scan_result(make_profile());
        app.adapt_screen
            .as_mut()
            .unwrap()
            .set_analyze_result(make_report());

        app.handle_data_event(TuiDataEvent::AdaptPlanResult(Ok(make_plan())));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Scaffolding);
        assert!(screen.results.plan.is_some());
        assert_eq!(app.pending_commands.len(), 1);
        assert!(matches!(
            app.pending_commands[0],
            TuiCommand::RunAdaptScaffold(_, _, _, _)
        ));
    }

    #[test]
    fn plan_ok_with_dry_run_completes() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Planning;
        app.adapt_screen.as_mut().unwrap().config.dry_run = true;

        app.handle_data_event(TuiDataEvent::AdaptPlanResult(Ok(make_plan())));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Complete);
        assert!(app.pending_commands.is_empty());
    }

    #[test]
    fn plan_err_sets_failed() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Planning;

        app.handle_data_event(TuiDataEvent::AdaptPlanResult(Err(anyhow::anyhow!(
            "plan failed"
        ))));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Failed);
        assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Planning);
    }

    #[test]
    fn materialize_ok_completes() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Materializing;

        app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Ok(
            make_materialize_result(),
        )));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Complete);
        assert!(screen.results.materialize.is_some());
    }

    #[test]
    fn materialize_err_sets_failed() {
        let mut app = app_with_adapt_screen();
        app.adapt_screen.as_mut().unwrap().step = AdaptStep::Materializing;

        app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Err(anyhow::anyhow!(
            "materialize failed"
        ))));

        let screen = app.adapt_screen.as_ref().unwrap();
        assert_eq!(screen.step, AdaptStep::Failed);
        assert_eq!(
            screen.error.as_ref().unwrap().phase,
            AdaptStep::Materializing
        );
    }

    #[test]
    fn cancelled_screen_ignores_scan_result() {
        let mut app = app_with_adapt_screen();
        let screen = app.adapt_screen.as_mut().unwrap();
        screen.cancelled = true;
        screen.results = crate::tui::screens::adapt::AdaptResults::default();

        app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));

        let screen = app.adapt_screen.as_ref().unwrap();
        // Step stays at Scanning (not transitioned)
        assert_eq!(screen.step, AdaptStep::Scanning);
        assert!(screen.results.profile.is_none());
        assert!(app.pending_commands.is_empty());
    }

    #[test]
    fn full_pipeline_happy_path() {
        let mut app = app_with_adapt_screen();

        // Phase 1: Scan
        app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));
        assert_eq!(
            app.adapt_screen.as_ref().unwrap().step,
            AdaptStep::Analyzing
        );

        // Phase 2: Analyze
        let cmd = app.pending_commands.pop().unwrap();
        assert!(matches!(cmd, TuiCommand::RunAdaptAnalyze(_, _)));
        app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Ok(make_report())));
        assert_eq!(
            app.adapt_screen.as_ref().unwrap().step,
            AdaptStep::Consolidating
        );

        // Phase 2.5: Consolidate (PRD)
        let cmd = app.pending_commands.pop().unwrap();
        assert!(matches!(cmd, TuiCommand::RunAdaptConsolidate(_, _, _)));
        app.handle_data_event(TuiDataEvent::AdaptConsolidateResult(Ok(
            "# PRD: Test".to_string()
        )));
        assert_eq!(app.adapt_screen.as_ref().unwrap().step, AdaptStep::Planning);

        // Phase 3: Plan
        let cmd = app.pending_commands.pop().unwrap();
        assert!(matches!(cmd, TuiCommand::RunAdaptPlan(_, _, _, _)));
        app.handle_data_event(TuiDataEvent::AdaptPlanResult(Ok(make_plan())));
        assert_eq!(
            app.adapt_screen.as_ref().unwrap().step,
            AdaptStep::Scaffolding
        );

        // Phase 3.5: Scaffold
        let cmd = app.pending_commands.pop().unwrap();
        assert!(matches!(cmd, TuiCommand::RunAdaptScaffold(_, _, _, _)));
        use crate::adapt::types::ScaffoldResult;
        app.handle_data_event(TuiDataEvent::AdaptScaffoldResult(Ok(ScaffoldResult {
            files: vec![],
            created_count: 0,
            skipped_count: 0,
        })));
        assert_eq!(
            app.adapt_screen.as_ref().unwrap().step,
            AdaptStep::Materializing
        );

        // Phase 4: Materialize
        let cmd = app.pending_commands.pop().unwrap();
        assert!(matches!(cmd, TuiCommand::RunAdaptMaterialize(_, _)));
        app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Ok(
            make_materialize_result(),
        )));
        assert_eq!(app.adapt_screen.as_ref().unwrap().step, AdaptStep::Complete);

        // All results stored
        let screen = app.adapt_screen.as_ref().unwrap();
        assert!(screen.results.profile.is_some());
        assert!(screen.results.report.is_some());
        assert!(screen.results.plan.is_some());
        assert!(screen.results.materialize.is_some());
    }

    #[test]
    fn home_screen_a_key_navigates_to_adapt_wizard() {
        use crate::tui::screens::home::{HomeScreen, ProjectInfo};
        use crate::tui::screens::test_helpers::key_event;
        use crate::tui::screens::{Screen, ScreenAction};
        use crossterm::event::KeyCode;

        let mut screen = HomeScreen::new(
            ProjectInfo {
                repo: "owner/repo".to_string(),
                branch: "main".to_string(),
                username: None,
            },
            vec![],
            vec![],
        );

        let action = screen.handle_input(
            &key_event(KeyCode::Char('a')),
            crate::tui::navigation::InputMode::Normal,
        );
        assert_eq!(action, ScreenAction::Push(TuiMode::AdaptWizard));
    }
}
