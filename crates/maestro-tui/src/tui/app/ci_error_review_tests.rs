//! Tests for the manual CI Error Review entry points (#695).
//! Split from `ci_error_review.rs` to keep that module under the
//! 400-line cap.

use crate::provider::github::ci::PendingPrCheck;
use crate::provider::types::{CheckConclusion, CheckRun, CheckStatus};
use crate::tui::app::types::{TuiCommand, TuiMode};
use crate::tui::screens::{CiFixConfig, FetchPhase};
use std::time::Instant;

fn pending(pr: u64, issue: u64, awaiting: bool) -> PendingPrCheck {
    PendingPrCheck {
        pr_number: pr,
        issue_number: issue,
        branch: format!("feat/issue-{}", issue),
        created_at: Instant::now(),
        check_count: 0,
        fix_attempt: 0,
        awaiting_fix_ci: awaiting,
    }
}

fn check(name: &str, conclusion: CheckConclusion) -> CheckRun {
    CheckRun {
        name: name.into(),
        status: CheckStatus::Completed,
        conclusion,
        started_at: None,
        elapsed_secs: Some(10),
    }
}

// ── Area C: has_visible_ci_failure / first_failing_pr ──────────────

#[test]
fn has_visible_ci_failure_returns_false_when_empty() {
    let app = crate::tui::make_test_app("issue-695-c1");
    assert!(!app.has_visible_ci_failure());
    assert!(app.first_failing_pr().is_none());
}

#[test]
fn has_visible_ci_failure_returns_false_when_all_success() {
    let mut app = crate::tui::make_test_app("issue-695-c2");
    app.ci_poller.add_check(pending(42, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(42, vec![check("clippy", CheckConclusion::Success)]);
    assert!(!app.has_visible_ci_failure());
    assert!(app.first_failing_pr().is_none());
}

#[test]
fn first_failing_pr_returns_lowest_when_multiple_fail() {
    let mut app = crate::tui::make_test_app("issue-695-c3");
    app.ci_poller.add_check(pending(200, 20, false));
    app.ci_poller.add_check(pending(100, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(200, vec![check("clippy", CheckConclusion::Failure)]);
    app.ci_poller
        .ci_check_details
        .insert(100, vec![check("test", CheckConclusion::Failure)]);
    let snap = app.first_failing_pr().expect("expected a snapshot");
    assert_eq!(snap.pr_number, 100, "lowest PR number must win");
    assert_eq!(snap.failed_check_names, vec!["test".to_string()]);
}

#[test]
fn has_visible_ci_failure_true_even_when_awaiting_fix() {
    let mut app = crate::tui::make_test_app("issue-695-c4");
    app.ci_poller.add_check(pending(55, 5, true));
    app.ci_poller
        .ci_check_details
        .insert(55, vec![check("test", CheckConclusion::Failure)]);
    assert!(app.has_visible_ci_failure());
    let snap = app.first_failing_pr().unwrap();
    assert!(snap.awaiting_fix_ci);
}

// ── Area D: request_ci_error_review ─────────────────────────────────

#[test]
fn request_ci_error_review_flips_mode_and_queues_fetch() {
    let mut app = crate::tui::make_test_app("issue-695-d1");
    app.ci_poller.add_check(pending(42, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(42, vec![check("clippy", CheckConclusion::Failure)]);
    app.tui_mode = TuiMode::Overview;
    app.request_ci_error_review();
    assert_eq!(app.tui_mode, TuiMode::CiErrorReview);
    assert!(app.screen_state.ci_error_review_screen.is_some());
    let s = app.screen_state.ci_error_review_screen.as_ref().unwrap();
    assert!(matches!(s.state.fetch, FetchPhase::Loading));
    assert_eq!(
        app.pending_commands.len(),
        1,
        "must queue a single FetchCiErrorReview command"
    );
    assert!(matches!(
        &app.pending_commands[0],
        TuiCommand::FetchCiErrorReview { pr_number: 42, .. }
    ));
}

#[test]
fn request_ci_error_review_idempotent_when_awaiting_fix() {
    let mut app = crate::tui::make_test_app("issue-695-d2");
    app.ci_poller.add_check(pending(42, 10, true));
    app.ci_poller
        .ci_check_details
        .insert(42, vec![check("clippy", CheckConclusion::Failure)]);
    app.tui_mode = TuiMode::Overview;
    app.request_ci_error_review();
    assert_ne!(
        app.tui_mode,
        TuiMode::CiErrorReview,
        "must NOT enter review mode while a fix is in flight"
    );
    assert!(app.pending_commands.is_empty());
}

#[test]
fn request_ci_error_review_sets_planned_gate_from_first_failed_check() {
    let mut app = crate::tui::make_test_app("issue-695-d3");
    app.ci_poller.add_check(pending(77, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(77, vec![check("clippy / lint", CheckConclusion::Failure)]);
    app.request_ci_error_review();
    let s = app.screen_state.ci_error_review_screen.as_ref().unwrap();
    assert_eq!(
        s.state.planned_gate_cmd.as_deref(),
        Some("cargo clippy --workspace --all-targets -- -D warnings")
    );
}

// ── Area E: handle_ci_error_review_fetched ──────────────────────────

#[test]
fn handle_ci_error_review_fetched_ok_sets_ready() {
    let mut app = crate::tui::make_test_app("issue-695-e1");
    app.ci_poller.add_check(pending(42, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(42, vec![check("clippy", CheckConclusion::Failure)]);
    app.request_ci_error_review();
    app.handle_ci_error_review_fetched(42, Ok("error: type mismatch".into()));
    let s = app.screen_state.ci_error_review_screen.as_ref().unwrap();
    match &s.state.fetch {
        FetchPhase::Ready { log_excerpt } => {
            assert_eq!(log_excerpt, "error: type mismatch");
        }
        other => panic!("expected Ready, got {:?}", other),
    }
    assert_eq!(app.tui_mode, TuiMode::CiErrorReview);
}

#[test]
fn handle_ci_error_review_fetched_err_sets_failed() {
    let mut app = crate::tui::make_test_app("issue-695-e2");
    app.ci_poller.add_check(pending(42, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(42, vec![check("clippy", CheckConclusion::Failure)]);
    app.request_ci_error_review();
    app.handle_ci_error_review_fetched(42, Err("gh: not found".into()));
    let s = app.screen_state.ci_error_review_screen.as_ref().unwrap();
    match &s.state.fetch {
        FetchPhase::Failed { reason } => assert_eq!(reason, "gh: not found"),
        other => panic!("expected Failed, got {:?}", other),
    }
    assert_eq!(app.tui_mode, TuiMode::CiErrorReview);
}

// ── Area J: [e] keybind via handle_key ──────────────────────────────

#[tokio::test]
async fn e_key_with_visible_failure_flips_to_ci_error_review_mode() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = crate::tui::make_test_app("issue-695-j1");
    app.ci_poller.add_check(pending(42, 10, false));
    app.ci_poller
        .ci_check_details
        .insert(42, vec![check("clippy", CheckConclusion::Failure)]);
    app.tui_mode = TuiMode::Overview;
    let _ = crate::tui::input_handler::handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
    )
    .await;
    assert_eq!(app.tui_mode, TuiMode::CiErrorReview);
}

#[tokio::test]
async fn e_key_with_no_failure_does_not_flip_mode() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = crate::tui::make_test_app("issue-695-j2");
    app.tui_mode = TuiMode::Overview;
    let _ = crate::tui::input_handler::handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
    )
    .await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Overview,
        "[e] must be a no-op when no failure is visible"
    );
}

// ── Area G: launch_ci_fix_from_review ───────────────────────────────

#[test]
fn launch_ci_fix_from_review_spawns_session_and_marks_awaiting() {
    let mut app = crate::tui::make_test_app("issue-695-g1");
    app.ci_poller.add_check(pending(42, 10, false));
    let config = CiFixConfig {
        pr_number: 42,
        issue_number: 10,
        branch: "feat/x".into(),
        local_gate_cmd: Some("cargo clippy --workspace --all-targets -- -D warnings".into()),
        failure_log: "boom".into(),
        attempt: 1,
    };
    app.launch_ci_fix_from_review(&config);
    assert_eq!(app.pending_session_launches.len(), 1);
    let prompt = &app.pending_session_launches[0].prompt;
    assert!(
        prompt.contains("Before pushing"),
        "prompt must inject the gate clause when local_gate_cmd is Some; got: {}",
        prompt
    );
    assert!(prompt.contains("cargo clippy"));
    let pending = app
        .ci_poller
        .pending_pr_checks
        .iter()
        .find(|c| c.pr_number == 42)
        .unwrap();
    assert!(pending.awaiting_fix_ci);
}
