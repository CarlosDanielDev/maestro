use super::super::helpers::build_gate_fix_prompt;
use crate::tui::app::*;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
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
        worktree_path: None,
        issue_number: Some(55),
        model: "opus".to_string(),
        agent_id: None,
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
        worktree_path: None,
        issue_number: None,
        model: "opus".to_string(),
        agent_id: None,
    };

    app.spawn_gate_fix_session(&line);
    assert!(
        app.pending_session_launches.is_empty(),
        "spawn_gate_fix_session must be a no-op when issue_number is None"
    );
}
