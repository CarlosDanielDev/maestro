use crate::tui::app::*;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
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
        worktree_path: None,
        issue_number: Some(1),
        model: "opus".to_string(),
        agent_id: None,
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
        worktree_path: None,
        issue_number: Some(42),
        model: "opus".to_string(),
        agent_id: None,
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
        worktree_path: None,
        issue_number: Some(7),
        model: "opus".to_string(),
        agent_id: None,
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
        None,
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
        None,
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
        None,
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
        None,
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
        None,
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
        None,
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
        None,
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
