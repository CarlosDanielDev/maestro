//! #1153 judge round 1 — the completion job must not race the rest of
//! the tick: no worktree teardown while its job runs, no two jobs for
//! one issue at once, and the auth-lost PR deferral keeps its old order.

#![cfg(test)]

use super::completion_off_loop_tests::{
    config_auto_pr_disabled, drive_until_applied, enqueue_pending_completion, make_app_with,
};
use crate::git::MockGitOps;
use crate::session::types::SessionStatus;
use std::time::Duration;

/// A gate-less session settles straight to COMPLETED when its stream
/// ends, and the same tick's finalize sweep tears COMPLETED worktrees
/// down. The sweep must skip the session while its job (backup, push)
/// still runs, and finalize it only after the outcome is applied.
#[tokio::test]
async fn issue_1153_no_teardown_while_job_in_flight() {
    let mut app = make_app_with(
        config_auto_pr_disabled(),
        MockGitOps::new().with_slow_push(1),
    );
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1160, "feat/issue-1160", wt.path().to_path_buf());
    app.pool.find_by_issue_mut(1160).unwrap().session.status = SessionStatus::Completed;
    assert!(app.pool.worktree_exists("issue-1160"), "precondition");

    app.check_completions().await.unwrap();
    assert!(
        app.pool.worktree_exists("issue-1160"),
        "finalize sweep must not remove the worktree while the job runs"
    );
    while !app.completions_in_flight.is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
        app.check_completions().await.unwrap();
        if !app.completions_in_flight.is_empty() {
            assert!(app.pool.worktree_exists("issue-1160"), "removed mid-job");
        }
    }

    assert!(
        app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("Pushed to branch feat/issue-1160")),
        "push must land before teardown"
    );
    assert!(
        !app.pool.worktree_exists("issue-1160"),
        "COMPLETED session is finalized (worktree torn down) once applied"
    );
}

/// Two completions for one issue must never run as two jobs at once —
/// both would run git in the same worktree. The second waits in
/// `pending_issue_completions` until the first is applied.
#[tokio::test]
async fn issue_1153_same_issue_jobs_do_not_overlap() {
    let mut app = make_app_with(
        config_auto_pr_disabled(),
        MockGitOps::new().with_slow_push(1),
    );
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1161, "feat/issue-1161", wt.path().to_path_buf());
    let again = app.pending_issue_completions[0].clone();
    app.pending_issue_completions.push(again);

    app.check_completions().await.unwrap();
    assert_eq!(app.completions_in_flight.len(), 1, "one job per issue");
    assert_eq!(app.pending_issue_completions.len(), 1, "second one waits");

    for _ in 0..2 {
        drive_until_applied(&mut app).await;
        app.check_completions().await.unwrap();
        assert!(app.completions_in_flight.len() <= 1, "jobs overlapped");
    }
    drive_until_applied(&mut app).await;
    assert!(app.pending_issue_completions.is_empty());
    let pushes = app
        .activity_log
        .entries()
        .iter()
        .filter(|e| e.message.contains("Pushed to branch feat/issue-1161"))
        .count();
    assert_eq!(pushes, 2, "both completions pushed, one after the other");
}

/// Pre-split order (#521): with auth lost, a successful session defers
/// its PR to the Shift+P queue before the `auto_pr` / client checks.
#[tokio::test]
async fn issue_1153_auth_lost_defers_pr_even_with_auto_pr_off() {
    let mut app = make_app_with(config_auto_pr_disabled(), MockGitOps::new());
    app.gh_auth_ok = false;
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1162, "feat/issue-1162", wt.path().to_path_buf());

    app.check_completions().await.unwrap();
    drive_until_applied(&mut app).await;

    assert!(
        app.pending_prs.iter().any(|p| p.issue_number == 1162),
        "auth-lost PR must be deferred to the Shift+P queue"
    );
}
