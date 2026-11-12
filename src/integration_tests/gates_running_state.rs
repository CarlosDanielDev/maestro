//! Pipeline-level regression tests for #1144. The stream handler must
//! park a gated session in `GATES_RUNNING` (`running → gates_running`)
//! instead of settling `COMPLETED` before the completion pipeline runs
//! the gates: `Completed` has no outgoing edges, so every pipeline
//! transition from it was silently rejected and the documented
//! `RUNNING → GATES_RUNNING → COMPLETED / FAILED_GATES` flow never
//! showed `GATES_RUNNING`.

use crate::integration_tests::helpers::{
    init_git_repo, make_app_with_gate, make_app_without_gates, make_session_with_issue,
};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus, StreamEvent};
use crate::tui::app::App;
use crate::tui::app::types::PendingIssueCompletion;
use std::path::Path;

fn make_completion(issue: u64, branch: &str, wt: &Path) -> PendingIssueCompletion {
    PendingIssueCompletion {
        issue_number: issue,
        issue_numbers: vec![],
        success: true,
        cost_usd: 0.0,
        files_touched: vec![],
        worktree_branch: Some(branch.to_string()),
        worktree_path: Some(wt.to_path_buf()),
        is_ci_fix: false,
    }
}

/// Arm a gated session the way production reaches the pipeline: promote
/// to RUNNING, set the gates flag (as `add_session` does), point
/// `worktree_path` at a real git repo, then fire the stream-completed
/// event through `ManagedSession::handle_event`.
fn arm_gated_session_at_stream_completed(app: &mut App, issue: u64, wt: &Path) {
    let session = make_session_with_issue(issue);
    let id = session.id;
    app.pool.enqueue(session);
    app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("active session");
    managed
        .session
        .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
        .unwrap();
    managed
        .session
        .transition_to(SessionStatus::Running, TransitionReason::Spawned)
        .unwrap();
    managed.session.gates_pending = true;
    managed.worktree_path = Some(wt.to_path_buf());
    managed.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });

    assert_eq!(
        managed.session.status,
        SessionStatus::GatesRunning,
        "stream completion must park a gated session in GATES_RUNNING, not settle COMPLETED (#1144)"
    );
}

/// Grab an owned copy of the session by issue — active or finished
/// bucket (the pipeline finalizes terminal sessions in the same tick).
fn find_session(app: &App, issue: u64) -> Session {
    app.pool
        .all_sessions()
        .into_iter()
        .find(|s| s.issue_number == Some(issue))
        .cloned()
        .expect("session must remain in the pool")
}

fn history_pair(session: &Session, from: SessionStatus, to: SessionStatus) -> Option<usize> {
    session
        .transition_history
        .iter()
        .position(|t| t.from == from && t.to == to)
}

/// Done when #1144 (1): with gates configured, `transition_history`
/// contains `running → gates_running` then `gates_running → completed`.
#[tokio::test]
async fn pipeline_records_running_to_gates_running_then_to_completed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let mut app = make_app_with_gate("issue-1144-gates-state", "true");
    arm_gated_session_at_stream_completed(&mut app, 1144, tmp.path());

    app.pending_issue_completions
        .push(make_completion(1144, "feat/issue-1144", tmp.path()));
    app.check_completions()
        .await
        .expect("check_completions must not error");

    let session = find_session(&app, 1144);
    let to_gates = history_pair(
        &session,
        SessionStatus::Running,
        SessionStatus::GatesRunning,
    )
    .expect("history must contain running → gates_running");
    let to_completed = history_pair(
        &session,
        SessionStatus::GatesRunning,
        SessionStatus::Completed,
    )
    .expect("history must contain gates_running → completed");
    assert!(
        to_gates < to_completed,
        "gates must run between RUNNING and COMPLETED"
    );
    assert_eq!(session.status, SessionStatus::Completed);

    assert!(
        !session
            .activity_log
            .iter()
            .any(|e| e.message.contains("TRANSITION REJECTED")),
        "no pipeline transition may be rejected on the happy path (#1144)"
    );
}

/// Done when #1144 (2): a failing required gate (command exits 1) ends
/// in `FAILED_GATES`. Settles the issue's Inferred note — the failure
/// branch previously started from COMPLETED, so its transition to
/// `FAILED_GATES` was silently rejected too.
#[tokio::test]
async fn failing_required_gate_ends_in_failed_gates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let mut app = make_app_with_gate("issue-1144-gates-fail", "false");
    arm_gated_session_at_stream_completed(&mut app, 1144, tmp.path());

    app.pending_issue_completions
        .push(make_completion(1144, "feat/issue-1144", tmp.path()));
    app.check_completions()
        .await
        .expect("check_completions must not error");

    let session = find_session(&app, 1144);
    assert_eq!(
        session.status,
        SessionStatus::FailedGates,
        "a failing required gate must settle FAILED_GATES, not COMPLETED (#1144)"
    );
    assert!(
        history_pair(
            &session,
            SessionStatus::GatesRunning,
            SessionStatus::FailedGates
        )
        .is_some(),
        "history must contain gates_running → failed_gates"
    );
    assert!(
        !session.gate_results.is_empty(),
        "failing gate results must be retained for recovery"
    );
}

/// Pin: a session that arrives at the pipeline still RUNNING (a path
/// that enqueues the completion without a stream-completed event) is
/// re-armed into GATES_RUNNING by the pipeline itself before the gates
/// execute.
#[tokio::test]
async fn pipeline_re_arms_running_session_into_gates_running() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let mut app = make_app_with_gate("issue-1144-gates-rearm", "true");
    let session = make_session_with_issue(1144);
    let id = session.id;
    app.pool.enqueue(session);
    app.pool.try_promote();
    {
        let managed = app.pool.get_active_mut(id).expect("active session");
        managed
            .session
            .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        managed
            .session
            .transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        managed.worktree_path = Some(tmp.path().to_path_buf());
    }

    app.pending_issue_completions
        .push(make_completion(1144, "feat/issue-1144", tmp.path()));
    app.check_completions()
        .await
        .expect("check_completions must not error");

    let session = find_session(&app, 1144);
    assert_eq!(session.status, SessionStatus::Completed);
    assert!(
        history_pair(
            &session,
            SessionStatus::Running,
            SessionStatus::GatesRunning
        )
        .is_some(),
        "pipeline must enter GATES_RUNNING before the gates run"
    );
}

/// Fill every pool slot with a decoy session so `add_session` cannot
/// promote (and therefore cannot spawn) the session under test. No
/// spawn loop runs here — `pool.try_promote` only moves sessions to
/// active.
fn fill_pool_slots(app: &mut App) {
    for i in 0..app.pool.max_concurrent() {
        let decoy = make_session_with_issue(9000 + i as u64);
        app.pool.enqueue(decoy);
        app.pool.try_promote();
    }
}

/// Wiring (#1144): `add_session` must flag gates-configured issue
/// sessions via `mark_gates_pending` — the stream handler parks only
/// flagged sessions in GATES_RUNNING.
#[tokio::test]
async fn add_session_flags_gated_issue_sessions() {
    let mut app = make_app_with_gate("issue-1144-flag-yes", "true");
    fill_pool_slots(&mut app);

    app.add_session(make_session_with_issue(1144))
        .await
        .unwrap();

    let session = find_session(&app, 1144);
    assert!(
        session.gates_pending,
        "gates configured + issue number → add_session must flag the session"
    );
}

/// Wiring (#1144), negative case: without gates configured the flag
/// must stay off, so the stream handler keeps settling COMPLETED.
#[tokio::test]
async fn add_session_does_not_flag_without_gates() {
    let mut app = make_app_without_gates("issue-1144-flag-no");
    fill_pool_slots(&mut app);

    app.add_session(make_session_with_issue(1144))
        .await
        .unwrap();

    let session = find_session(&app, 1144);
    assert!(!session.gates_pending);
}

fn find_session_by_id(app: &App, id: uuid::Uuid) -> Session {
    app.pool
        .all_sessions()
        .into_iter()
        .find(|s| s.id == id)
        .cloned()
        .expect("session must remain in the pool")
}

/// Regression (#1144 fix round): the context-overflow fork child is the
/// one production enqueue that bypasses `add_session` — it must be
/// flagged too, or a gated fork child settles COMPLETED at stream
/// completion and a failing required gate never surfaces as
/// FAILED_GATES.
#[tokio::test]
async fn gated_fork_child_with_failing_gate_ends_in_failed_gates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let mut app = make_app_with_gate("issue-1144-fork-child", "false");
    app.fork_policy = Some(crate::session::fork::ForkPolicy::new(5));

    // Parent at RUNNING with an issue, context above the overflow ratio.
    let parent = make_session_with_issue(1144);
    let parent_id = parent.id;
    app.pool.enqueue(parent);
    app.pool.try_promote();
    {
        let managed = app.pool.get_active_mut(parent_id).expect("active parent");
        managed
            .session
            .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        managed
            .session
            .transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
    }
    app.context_monitor.record_context(parent_id, 0.95);

    app.check_context_overflow(parent_id);

    // Fork happened: the child inherits the issue and must be flagged.
    let child = app
        .pool
        .all_sessions()
        .into_iter()
        .find(|s| s.parent_session_id == Some(parent_id))
        .cloned()
        .expect("fork child must exist in the pool");
    assert_eq!(child.issue_number, Some(1144), "child inherits the issue");
    assert!(
        child.gates_pending,
        "fork child bypasses add_session — it must be flagged for gates here (#1144)"
    );

    // The parent settled Completed by the fork; free its slot for the
    // child (the pipeline would finalize it later anyway).
    app.pool.finalize(parent_id);

    // Drive the child through the real completion pipeline.
    let child_id = child.id;
    app.pool.try_promote();
    {
        let managed = app.pool.get_active_mut(child_id).expect("active child");
        managed
            .session
            .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        managed
            .session
            .transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        managed.worktree_path = Some(tmp.path().to_path_buf());
        managed.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });
        assert_eq!(
            managed.session.status,
            SessionStatus::GatesRunning,
            "gated fork child must park in GATES_RUNNING at stream completion (#1144)"
        );
    }
    app.pending_issue_completions
        .push(make_completion(1144, "feat/issue-1144", tmp.path()));
    app.check_completions()
        .await
        .expect("check_completions must not error");

    let child = find_session_by_id(&app, child_id);
    assert_eq!(
        child.status,
        SessionStatus::FailedGates,
        "a failing required gate must settle the gated fork child FAILED_GATES (#1144)"
    );
    assert!(
        history_pair(
            &child,
            SessionStatus::GatesRunning,
            SessionStatus::FailedGates
        )
        .is_some(),
        "history must contain gates_running → failed_gates"
    );
}
