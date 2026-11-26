//! #1469: Kill teardown never loses committed work. The session runs in a
//! real git worktree; `finalize` removes the worktree and deletes the
//! branch only when no commit lives on that branch alone.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use uuid::Uuid;

use super::SessionPool;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;

fn git(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be on PATH")
        .status
        .success()
}

/// A git repo with one commit on `main`, no remote.
fn repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("tempdir");
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "ci@example.com"],
        &["config", "user.name", "CI"],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        assert!(git(repo.path(), args), "git {args:?}");
    }
    repo
}

/// A running session for `issue` in a real worktree of `repo`.
fn running_in(repo: &Path, issue: u64) -> (SessionPool, Uuid, PathBuf) {
    let (tx, _rx) = mpsc::unbounded_channel();
    let worktrees = Box::new(GitWorktreeManager::new(repo.to_path_buf()));
    let mut pool = SessionPool::new(3, worktrees, tx);
    let session = Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    let id = session.id;
    pool.enqueue(session);
    let _ = pool.try_promote();
    let managed = pool.get_active_mut(id).expect("promoted");
    let worktree = managed.worktree_path.clone().expect("a real worktree");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).expect("transition");
    }
    (pool, id, worktree)
}

/// Kill the session and run the real teardown.
fn kill_and_finalize(pool: &mut SessionPool, id: Uuid) {
    let managed = pool.get_active_mut(id).expect("active");
    (managed.session)
        .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
        .expect("kill");
    assert!(pool.finalize(id), "the killed session is finalized");
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    git(repo, &["rev-parse", "--verify", "-q", branch])
}

#[test]
fn teardown_keeps_branch_with_unpushed_commits() {
    let repo = repo();
    let (mut pool, id, worktree) = running_in(repo.path(), 1469);
    assert!(git(
        &worktree,
        &["commit", "-q", "--allow-empty", "-m", "agent work"]
    ));

    kill_and_finalize(&mut pool, id);

    assert!(!worktree.exists(), "the worktree is still removed");
    assert!(
        branch_exists(repo.path(), "maestro/issue-1469"),
        "the branch with the unpushed commit is kept"
    );
}

#[test]
fn teardown_removes_branch_without_unique_commits() {
    let repo = repo();
    let (mut pool, id, worktree) = running_in(repo.path(), 1470);

    kill_and_finalize(&mut pool, id);

    assert!(!worktree.exists(), "the worktree is removed");
    assert!(
        !branch_exists(repo.path(), "maestro/issue-1470"),
        "a branch with nothing of its own is deleted, as before"
    );
}
