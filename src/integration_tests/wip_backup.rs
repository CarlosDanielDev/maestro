//! Pipeline-level regression tests for #562. Exercise
//! `App::check_completions` against a real git tempdir (no
//! `MockGitOps`) so the WIP commit ends up as a verifiable `git log`
//! entry — matching the AC #7 / AC #8 contracts.

use crate::git::{CliGitOps, GitOps, WIP_SUBJECT_PREFIX, WIP_SUBJECT_SUFFIX};
use crate::integration_tests::helpers::{
    git_head_subject, init_git_repo, make_app_with_gate, make_session_with_issue, run_git,
};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::app::App;
use crate::tui::app::types::PendingIssueCompletion;
use std::path::Path;
use std::process::Command;

fn count_wip_commits(dir: &Path) -> usize {
    let out = Command::new("git")
        .args(["log", "--all", "--pretty=%s"])
        .current_dir(dir)
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with(WIP_SUBJECT_PREFIX) && l.ends_with(WIP_SUBJECT_SUFFIX))
        .count()
}

/// Bare remote + working clone, branched off `main` to `branch`. Both
/// tempdirs returned so callers keep them alive for the test.
fn build_remote_and_clone(branch: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let remote = tempfile::tempdir().expect("remote tempdir");
    let s = Command::new("git")
        .args(["init", "--bare", "-q", "-b", "main"])
        .current_dir(remote.path())
        .status()
        .expect("git init bare");
    assert!(s.success());

    let work = tempfile::tempdir().expect("work tempdir");
    let s = Command::new("git")
        .args([
            "clone",
            "-q",
            remote.path().to_str().unwrap(),
            work.path().to_str().unwrap(),
        ])
        .status()
        .expect("git clone");
    assert!(s.success());

    run_git(work.path(), &["config", "user.email", "test@example.com"]);
    run_git(work.path(), &["config", "user.name", "Test"]);
    std::fs::write(work.path().join("README.md"), "init").unwrap();
    run_git(work.path(), &["add", "README.md"]);
    run_git(work.path(), &["commit", "-q", "-m", "init"]);
    run_git(work.path(), &["push", "-u", "origin", "main"]);
    run_git(work.path(), &["checkout", "-q", "-b", branch]);
    run_git(work.path(), &["push", "-u", "origin", branch]);

    (remote, work)
}

/// Promote a session for `issue` and walk it through Spawning →
/// Running so the next `check_completions` tick can transition into
/// GatesRunning. Pushes the supplied `PendingIssueCompletion`.
fn arm_session_at_running(app: &mut App, issue: u64, completion: PendingIssueCompletion) {
    let session = make_session_with_issue(issue);
    let id = session.id;
    app.pool.enqueue(session);
    app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("active");
    managed
        .session
        .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
        .unwrap();
    managed
        .session
        .transition_to(SessionStatus::Running, TransitionReason::Spawned)
        .unwrap();
    app.pending_issue_completions.push(completion);
}

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

/// AC #7 — gate failure must leave the WIP backup commit on the branch
/// as the recovery surface. Pipeline-level regression test.
#[tokio::test]
async fn pipeline_wip_backup_commit_survives_gate_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());
    std::fs::write(tmp.path().join("model_edit.rs"), "fn model_work() {}").unwrap();

    let mut app = make_app_with_gate("issue-562-wip-fail", "false");
    arm_session_at_running(
        &mut app,
        562,
        make_completion(562, "feat/issue-562", tmp.path()),
    );

    app.check_completions()
        .await
        .expect("check_completions must not error");

    let subject = git_head_subject(tmp.path());
    let expected = format!("{}{}{}", WIP_SUBJECT_PREFIX, 562, WIP_SUBJECT_SUFFIX);
    assert_eq!(
        subject, expected,
        "WIP commit must be HEAD after gate failure (AC #7) — model edits must survive"
    );

    let tree_out = Command::new("git")
        .args(["show", "--name-only", "--pretty=", "HEAD"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let tree_files = String::from_utf8_lossy(&tree_out.stdout);
    assert!(
        tree_files.contains("model_edit.rs"),
        "model_edit.rs must be part of the WIP commit tree, got:\n{}",
        tree_files
    );
}

/// AC #3 + AC #6 — gate pass must amend the WIP into a clean conventional
/// commit; HEAD subject is the conventional message, not the WIP one.
#[tokio::test]
async fn pipeline_success_path_head_has_clean_commit_message() {
    let branch = "feat/issue-562";
    let (_remote, work) = build_remote_and_clone(branch);
    std::fs::write(work.path().join("feature.rs"), "fn feature() {}").unwrap();

    let mut app = make_app_with_gate("issue-562-wip-pass", "true");
    arm_session_at_running(&mut app, 562, make_completion(562, branch, work.path()));

    app.check_completions()
        .await
        .expect("check_completions must not error");

    let subject = git_head_subject(work.path());
    assert_eq!(
        subject, "feat: implement changes for issue #562",
        "success path: HEAD must hold the clean conventional-commit message after amend"
    );
    assert!(
        !subject.starts_with(WIP_SUBJECT_PREFIX),
        "WIP prefix must not appear in HEAD subject on success path"
    );
}

/// AC #8 — second pipeline run on a branch that already has a WIP backup
/// commit at HEAD must NOT stack a second WIP. Resume idempotency.
#[tokio::test]
async fn pipeline_does_not_stack_wip_commits_on_second_gate_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_git_repo(tmp.path());

    let ops = CliGitOps;
    let pre_ops: &dyn GitOps = &ops;
    pre_ops.backup_wip(tmp.path(), 562).expect("seed first WIP");
    assert_eq!(
        count_wip_commits(tmp.path()),
        1,
        "precondition: exactly one WIP commit"
    );

    let mut app = make_app_with_gate("issue-562-wip-resume", "false");
    arm_session_at_running(
        &mut app,
        562,
        make_completion(562, "feat/issue-562", tmp.path()),
    );

    app.check_completions()
        .await
        .expect("check_completions must not error");

    assert_eq!(
        count_wip_commits(tmp.path()),
        1,
        "AC #8: must remain exactly ONE WIP commit (head_is_wip_backup short-circuit)"
    );
}
