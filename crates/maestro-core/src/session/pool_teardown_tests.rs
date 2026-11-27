//! #1469: Kill teardown never loses committed work. The session runs in a
//! real git worktree; `finalize` removes the worktree and deletes the
//! branch only when no commit lives on that branch alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::mpsc;
use uuid::Uuid;

use super::SessionPool;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::{GitWorktreeManager, WorktreeManager};

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
    running_with(Box::new(GitWorktreeManager::new(repo.to_path_buf())), issue)
}

/// As [`running_in`], over the given worktree manager.
fn running_with(worktrees: Box<dyn WorktreeManager>, issue: u64) -> (SessionPool, Uuid, PathBuf) {
    let (tx, _rx) = mpsc::unbounded_channel();
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
    let worktree = start(&mut pool, id);
    (pool, id, worktree)
}

/// Move a promoted session to `Running`; returns its worktree.
fn start(pool: &mut SessionPool, id: Uuid) -> PathBuf {
    let managed = pool.get_active_mut(id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).expect("transition");
    }
    managed.worktree_path.clone().expect("a real worktree")
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

/// #1485: the parent errored and its retry was spawned (`Retrying`).
fn mark_retrying(pool: &mut SessionPool, id: Uuid) {
    let managed = pool.get_active_mut(id).expect("active");
    for (to, why) in [
        (SessionStatus::Errored, TransitionReason::StreamError),
        (SessionStatus::Retrying, TransitionReason::RetryTriggered),
    ] {
        managed.session.transition_to(to, why).expect("transition");
    }
}

/// The retry child: a new session for the same issue, so the same slug.
fn retry_child(issue: u64) -> Session {
    Session::new(
        "probe retry".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    )
}

#[test]
fn finalize_retrying_keeps_worktree_for_retry_child() {
    let repo = repo();
    let (mut pool, id, worktree) = running_in(repo.path(), 1485);
    mark_retrying(&mut pool, id);
    let child = retry_child(1485);
    let child_id = child.id;
    pool.enqueue(child);

    assert!(pool.finalize(id), "the Retrying parent is finalized");

    assert!(
        worktree.exists(),
        "the queued retry child's worktree is kept"
    );
    assert_eq!(pool.try_promote(), vec![child_id]);
    assert_eq!(
        pool.get_active(child_id).expect("promoted").worktree_path,
        Some(worktree),
        "the retry child runs in the kept worktree, not in the cwd"
    );
}

#[test]
fn retry_child_promoted_before_parent_finalizes_shares_worktree() {
    let repo = repo();
    let (mut pool, id, worktree) = running_in(repo.path(), 1486);
    mark_retrying(&mut pool, id);
    let child = retry_child(1486);
    let child_id = child.id;
    // Same tick as the completion pipeline: the child is enqueued and
    // promoted while the parent is still active and `Retrying`.
    pool.enqueue(child);
    assert_eq!(pool.try_promote(), vec![child_id]);

    assert!(pool.finalize(id), "the Retrying parent is finalized");

    assert!(
        worktree.exists(),
        "the active retry child's worktree is kept"
    );
    assert_eq!(
        pool.get_active(child_id).expect("active").worktree_path,
        Some(worktree),
        "the retry child runs in the parent's worktree, not in the cwd"
    );
}

#[test]
fn finalize_retrying_without_child_tears_down() {
    let repo = repo();
    let (mut pool, id, worktree) = running_in(repo.path(), 1487);
    mark_retrying(&mut pool, id);

    assert!(pool.finalize(id), "the Retrying parent is finalized");

    assert!(!worktree.exists(), "no child shares the slug: torn down");
}

/// #1505: a Running session stalls and its retry is triggered.
fn mark_stalled_retrying(pool: &mut SessionPool, id: Uuid) {
    let managed = pool.get_active_mut(id).expect("active");
    for (to, why) in [
        (SessionStatus::Stalled, TransitionReason::HealthStall),
        (SessionStatus::Retrying, TransitionReason::RetryTriggered),
    ] {
        managed.session.transition_to(to, why).expect("transition");
    }
}

#[test]
fn stalled_parent_is_cancelled_before_retry_child_spawns() {
    let repo = repo();
    let (mut pool, id, worktree) = running_in(repo.path(), 1505);
    // Fake provider process: the cancel token a spawn would arm.
    let parent_turn = pool.get_active_mut(id).expect("active").arm_turn_for_test();
    mark_stalled_retrying(&mut pool, id);
    let child = retry_child(1505);
    let child_id = child.id;

    // Same tick as the completion pipeline: the child is promoted (and
    // would be spawned) while the parent is still active and `Retrying`.
    pool.enqueue(child);
    assert_eq!(pool.try_promote(), vec![child_id]);

    assert!(
        parent_turn.is_cancelled(),
        "the stalled parent's process is stopped before the child spawns"
    );
    assert_eq!(
        pool.get_active(child_id).expect("promoted").worktree_path,
        Some(worktree),
        "the child still runs in the shared worktree"
    );
}

/// Counts `remove` calls on top of the real git worktree manager.
struct CountingWorktrees {
    git: GitWorktreeManager,
    removes: Arc<AtomicUsize>,
}

impl WorktreeManager for CountingWorktrees {
    fn create(&self, slug: &str) -> anyhow::Result<PathBuf> {
        self.git.create(slug)
    }
    fn remove(&self, slug: &str) -> anyhow::Result<()> {
        self.removes.fetch_add(1, Ordering::SeqCst);
        self.git.remove(slug)
    }
    fn exists(&self, slug: &str) -> bool {
        self.git.exists(slug)
    }
}

#[test]
fn finalize_retry_child_tears_down_shared_worktree_once() {
    let repo = repo();
    let removes = Arc::new(AtomicUsize::new(0));
    let worktrees = Box::new(CountingWorktrees {
        git: GitWorktreeManager::new(repo.path().to_path_buf()),
        removes: Arc::clone(&removes),
    });
    let (mut pool, id, worktree) = running_with(worktrees, 1505);
    mark_stalled_retrying(&mut pool, id);
    let child = retry_child(1505);
    let child_id = child.id;
    pool.enqueue(child);
    assert_eq!(pool.try_promote(), vec![child_id]);
    assert!(pool.finalize(id), "the Retrying parent is finalized");
    assert_eq!(
        removes.load(Ordering::SeqCst),
        0,
        "the child still needs it"
    );
    assert_eq!(start(&mut pool, child_id), worktree, "one shared worktree");

    // The child commits work it never pushes, then is killed.
    assert!(git(
        &worktree,
        &["commit", "-q", "--allow-empty", "-m", "retry work"]
    ));
    kill_and_finalize(&mut pool, child_id);

    assert_eq!(removes.load(Ordering::SeqCst), 1, "torn down exactly once");
    assert!(!worktree.exists(), "the shared worktree is removed");
    assert!(
        branch_exists(repo.path(), "maestro/issue-1505"),
        "the branch with the unpushed commit is kept"
    );
}
