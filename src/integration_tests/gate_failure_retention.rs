//! Issue #558: post-completion gate failure must NOT tear down the worktree.
//!
//! These tests use real `git worktree` commands (not the `MockWorktreeManager`)
//! so the regression we're guarding — a real `git worktree remove --force`
//! call landing on the gate-failure path — is covered end to end.

use crate::integration_tests::helpers::make_session_with_issue;
use crate::session::pool::SessionPool;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;
use std::path::Path;
use std::process::Command;
use tokio::sync::mpsc;

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git must be on PATH");
    assert!(status.success(), "git {:?} failed in {:?}", args, dir);
}

fn init_git_repo(dir: &Path) {
    run_git(dir, &["init", "-q", "-b", "main"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join("README.md"), "init").expect("write README");
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "-q", "-m", "init"]);
}

fn promote_one(pool: &mut SessionPool, session: Session) -> uuid::Uuid {
    let id = session.id;
    pool.enqueue(session);
    pool.try_promote();
    id
}

#[test]
fn finalize_retain_worktree_keeps_real_git_worktree_and_uncommitted_file() {
    // The literal bug-reproducer from session #542: model edits in the
    // worktree, never commits, gate fails, expect the worktree (and its
    // uncommitted file) to survive.
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let wt_mgr = GitWorktreeManager::new(tmp.path().to_path_buf());
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut pool = SessionPool::new(1, Box::new(wt_mgr), tx);
    let id = promote_one(&mut pool, make_session_with_issue(558));

    let wt_path = tmp.path().join(".maestro/worktrees/issue-558");
    assert!(
        wt_path.exists(),
        "precondition: try_promote must create the real worktree"
    );

    let model_file = wt_path.join("model_work.rs");
    std::fs::write(&model_file, "fn pretend_model_wrote_this() {}")
        .expect("write uncommitted model file");

    pool.finalize_retain_worktree(id);

    assert!(
        wt_path.exists(),
        "real worktree directory must survive finalize_retain_worktree"
    );
    assert!(
        model_file.exists(),
        "uncommitted file inside the worktree must survive"
    );
    let contents = std::fs::read_to_string(&model_file).expect("read model file");
    assert_eq!(
        contents, "fn pretend_model_wrote_this() {}",
        "uncommitted model content must be intact byte-for-byte"
    );
}

#[test]
fn finalize_and_teardown_removes_real_git_worktree() {
    // Counterpart sanity: the success path (explicit teardown) must still
    // tear down the real worktree, otherwise we'd silently leak directories
    // forever after the fix.
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let wt_mgr = GitWorktreeManager::new(tmp.path().to_path_buf());
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut pool = SessionPool::new(1, Box::new(wt_mgr), tx);
    let id = promote_one(&mut pool, make_session_with_issue(558));

    let wt_path = tmp.path().join(".maestro/worktrees/issue-558");
    assert!(wt_path.exists(), "precondition: worktree was created");

    pool.finalize_and_teardown(id);

    assert!(
        !wt_path.exists(),
        "real worktree must be removed by finalize_and_teardown (success path)"
    );
}

#[tokio::test]
async fn check_completions_retains_worktree_when_terminal_status_is_failed_gates() {
    // Pipeline-level proof of the bug fix: when the dispatcher in
    // `check_completions` sees a terminal session whose status is
    // `FailedGates`, it must route to `finalize_retain_worktree` (keep the
    // worktree), NOT `finalize_and_teardown` (delete it).
    let mut app = crate::tui::make_test_app("issue-558-pipeline-fail");

    let session = make_session_with_issue(558);
    let id = session.id;
    app.pool.enqueue(session);
    app.pool.try_promote();
    assert!(
        app.pool.worktree_exists("issue-558"),
        "precondition: worktree was created on promote"
    );

    // Walk the state machine: Queued -> Spawning -> Running -> GatesRunning -> FailedGates.
    let managed = app
        .pool
        .get_active_mut(id)
        .expect("session is active after promote");
    managed
        .session
        .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
        .expect("Queued -> Spawning");
    managed
        .session
        .transition_to(SessionStatus::Running, TransitionReason::Spawned)
        .expect("Spawning -> Running");
    managed
        .session
        .transition_to(SessionStatus::GatesRunning, TransitionReason::GatesStarted)
        .expect("Running -> GatesRunning");
    managed
        .session
        .transition_to(SessionStatus::FailedGates, TransitionReason::GatesFailed)
        .expect("GatesRunning -> FailedGates");

    app.check_completions()
        .await
        .expect("check_completions must not error");

    assert!(
        app.pool.worktree_exists("issue-558"),
        "worktree must be retained when terminal status is FailedGates"
    );
    let session_after = app
        .pool
        .get_session(id)
        .expect("session must be findable in finished bucket");
    assert_eq!(session_after.status, SessionStatus::FailedGates);
}

#[tokio::test]
async fn check_completions_tears_down_worktree_when_terminal_status_is_completed() {
    // Counterpart sanity at the pipeline level: success path still tears down.
    let mut app = crate::tui::make_test_app("issue-558-pipeline-success");

    let session = make_session_with_issue(558);
    let id = session.id;
    app.pool.enqueue(session);
    app.pool.try_promote();
    assert!(app.pool.worktree_exists("issue-558"));

    let managed = app
        .pool
        .get_active_mut(id)
        .expect("session is active after promote");
    managed
        .session
        .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
        .expect("Queued -> Spawning");
    managed
        .session
        .transition_to(SessionStatus::Running, TransitionReason::Spawned)
        .expect("Spawning -> Running");
    managed
        .session
        .transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
        .expect("Running -> Completed");

    app.check_completions()
        .await
        .expect("check_completions must not error");

    assert!(
        !app.pool.worktree_exists("issue-558"),
        "worktree MUST be torn down when terminal status is Completed"
    );
}

#[test]
fn gate_failure_transition_to_failed_gates_is_valid() {
    // Lock the state-machine transition the pipeline relies on.
    let mut session = make_session_with_issue(558);
    session
        .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
        .expect("Queued -> Spawning");
    session
        .transition_to(SessionStatus::Running, TransitionReason::Spawned)
        .expect("Spawning -> Running");
    session
        .transition_to(SessionStatus::GatesRunning, TransitionReason::GatesStarted)
        .expect("Running -> GatesRunning");
    session
        .transition_to(SessionStatus::FailedGates, TransitionReason::GatesFailed)
        .expect("GatesRunning -> FailedGates must be a valid transition");

    assert_eq!(session.status, SessionStatus::FailedGates);
    assert!(
        session.status.is_terminal(),
        "FailedGates must be terminal so the dispatcher routes it to \
         finalize_retain_worktree"
    );
}
