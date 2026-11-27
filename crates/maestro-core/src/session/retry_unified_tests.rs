//! #1506: a unified session (several issues, one session) keeps its
//! worktree across a retry. The child built by `prepare_retry` must share
//! the parent's slug, or `finalize` tears the parent's worktree down.
//! Fixtures mirror `pool_teardown_tests.rs` (real git in a tempdir).

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use uuid::Uuid;

use super::RetryPolicy;
use crate::config::HollowRetryConfig;
use crate::session::pool::SessionPool;
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

/// A running unified session for `issues` in a real worktree of `repo`,
/// built the way the TUI builds one: primary issue + `issue_numbers`.
fn running_unified_in(repo: &Path, issues: &[u64]) -> (SessionPool, Uuid, PathBuf) {
    let (tx, _rx) = mpsc::unbounded_channel();
    let worktrees = Box::new(GitWorktreeManager::new(repo.to_path_buf()));
    let mut pool = SessionPool::new(3, worktrees, tx);
    let mut session = Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        issues.first().copied(),
        None,
    );
    session.issue_numbers = issues.to_vec();
    let id = session.id;
    pool.enqueue(session);
    let _ = pool.try_promote();
    let managed = pool.get_active_mut(id).expect("promoted");
    let worktree = managed.worktree_path.clone().expect("a real worktree");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::Errored, TransitionReason::StreamError),
        (SessionStatus::Retrying, TransitionReason::RetryTriggered),
    ] {
        managed.session.transition_to(to, why).expect("transition");
    }
    (pool, id, worktree)
}

#[test]
fn finalize_retrying_unified_parent_keeps_worktree_for_child() {
    let repo = repo();
    let (mut pool, id, worktree) = running_unified_in(repo.path(), &[1506, 1507]);
    std::fs::write(worktree.join("wip.txt"), "uncommitted").expect("write");
    let parent = pool.get_active(id).expect("active").session.clone();
    let policy = RetryPolicy::new(2, 0, HollowRetryConfig::default());
    let child = policy.prepare_retry(&parent, None, None);
    let child_id = child.id;
    pool.enqueue(child);

    assert!(pool.finalize(id), "the Retrying parent is finalized");

    assert!(
        worktree.join("wip.txt").exists(),
        "the parent's uncommitted work survives for the retry child"
    );
    assert_eq!(pool.try_promote(), vec![child_id]);
    assert_eq!(
        pool.get_active(child_id).expect("promoted").worktree_path,
        Some(worktree),
        "the retry child runs in the parent's worktree"
    );
}
