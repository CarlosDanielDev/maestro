use crate::session::types::GateResultEntry;
use crate::tui::app::*;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
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
        worktree_path: None,
        issue_number: Some(42),
        model: "opus".to_string(),
        agent_id: None,
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
        worktree_path: None,
        issue_number: Some(7),
        model: "opus".to_string(),
        agent_id: None,
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
            worktree_path: None,
            issue_number: Some(1),
            model: "opus".to_string(),
            agent_id: None,
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
            worktree_path: None,
            issue_number: Some(2),
            model: "opus".to_string(),
            agent_id: None,
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
                worktree_path: None,
                issue_number: Some(1),
                model: "opus".to_string(),
                agent_id: None,
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
                worktree_path: None,
                issue_number: Some(2),
                model: "opus".to_string(),
                agent_id: None,
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
        None,
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
        None,
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
        None,
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
        None,
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
        None,
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
        None,
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
        None,
    );
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(summary.sessions[0].model, "claude-opus-4");
}
