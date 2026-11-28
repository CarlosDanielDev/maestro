//! #1556: Kill on a queued retry child (`dequeue`) never leaves the
//! worktree its finished parent handed it behind, and never removes a
//! worktree a live session still runs in.

use std::path::PathBuf;

use tokio::sync::mpsc;
use uuid::Uuid;

use super::tests::{make_pool, make_session};
use super::{Dequeued, SessionPool};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::{MockWorktreeManager, WorktreeManager};

/// A session for `issue`: every session of one issue shares its slug.
fn for_issue(issue: u64) -> Session {
    let mut session = make_session("probe");
    session.issue_number = Some(issue);
    session
}

/// Move the active session `id` through `steps`.
fn walk(pool: &mut SessionPool, id: Uuid, steps: &[(SessionStatus, TransitionReason)]) {
    let managed = pool.get_active_mut(id).expect("active");
    for (to, why) in steps {
        managed
            .session
            .transition_to(*to, *why)
            .expect("transition");
    }
}

const RUNNING: [(SessionStatus, TransitionReason); 2] = [
    (SessionStatus::Spawning, TransitionReason::Promoted),
    (SessionStatus::Running, TransitionReason::Spawned),
];

const RETRYING: [(SessionStatus, TransitionReason); 2] = [
    (SessionStatus::Stalled, TransitionReason::HealthStall),
    (SessionStatus::Retrying, TransitionReason::RetryTriggered),
];

/// A running session for `issue` in a (fake) worktree.
fn running(pool: &mut SessionPool, issue: u64) -> Uuid {
    let session = for_issue(issue);
    let id = session.id;
    pool.enqueue(session);
    assert_eq!(pool.try_promote(), vec![id]);
    walk(pool, id, &RUNNING);
    id
}

#[test]
fn dequeue_retry_child_with_handed_worktree_tears_it_down() {
    let mut pool = make_pool(1);
    let parent = running(&mut pool, 1556);
    walk(&mut pool, parent, &RETRYING);
    let child = for_issue(1556);
    let child_id = child.id;
    pool.enqueue(child);
    assert!(pool.finalize(parent), "the Retrying parent is finalized");
    assert!(pool.worktree_exists("issue-1556"), "handed to the child");

    assert!(pool.dequeue(child_id), "the child leaves the queue");

    assert!(
        !pool.worktree_exists("issue-1556"),
        "no session is left on the handed worktree: it is torn down"
    );
}

#[test]
fn dequeue_child_of_running_parent_keeps_worktree() {
    let mut pool = make_pool(2);
    let first = running(&mut pool, 1557);
    walk(&mut pool, first, &RETRYING);
    // The first retry runs in the shared worktree (#1485) …
    let parent = running(&mut pool, 1557);
    // … and its own retry waits in the queue (both slots are taken).
    let child = for_issue(1557);
    let child_id = child.id;
    pool.enqueue(child);
    assert!(pool.finalize(first), "the first session is finalized");

    assert!(pool.dequeue(child_id), "the child leaves the queue");

    assert!(
        pool.worktree_exists("issue-1557"),
        "its parent still runs there: the worktree stays"
    );
    assert!(pool.get_active(parent).is_some(), "the parent still runs");
}

/// A session paused by quit comes back queued with its own worktree
/// (#1266). Nobody handed it that worktree: removing it from the queue
/// never deletes its files.
#[test]
fn dequeue_keeps_the_worktree_of_a_resumed_session() {
    let mut pool = make_pool(1);
    let before = running(&mut pool, 1558);
    pool.finalize_retain_worktree(before);
    let mut resumed = for_issue(1558);
    resumed.worktree_path = Some("/tmp/mock-worktrees/issue-1558".into());
    let resumed_id = resumed.id;
    pool.enqueue(resumed);

    assert!(pool.dequeue(resumed_id), "it leaves the queue");

    assert!(pool.worktree_exists("issue-1558"), "its worktree stays");
}

/// #1556 judge: the dialog opened while the parent still ran ("no worktree
/// is touched"); the parent handed its worktree over before `⏎`. The
/// worktree stays, as the dialog said, and the caller is told.
#[test]
fn dequeue_as_shown_keeps_a_worktree_the_dialog_said_stays() {
    let mut pool = make_pool(1);
    let parent = running(&mut pool, 1559);
    walk(&mut pool, parent, &RETRYING);
    let child = for_issue(1559);
    let child_id = child.id;
    pool.enqueue(child);
    let shown = pool.dequeue_tears_down(child_id);
    assert!(!shown, "the dialog opens before the hand-over");
    assert!(pool.finalize(parent), "the parent hands its worktree over");

    let outcome = pool.dequeue_as_shown(child_id, shown);

    assert_eq!(outcome, Some(Dequeued::KeptAsShown));
    assert!(pool.worktree_exists("issue-1559"), "as the dialog said");
}

/// A worktree manager whose teardown does nothing and says `Ok`, as
/// `GitWorktreeManager::remove` does when git fails or refuses.
struct StuckWorktrees(MockWorktreeManager);

impl WorktreeManager for StuckWorktrees {
    fn create(&self, slug: &str) -> anyhow::Result<PathBuf> {
        self.0.create(slug)
    }
    fn remove(&self, _slug: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn exists(&self, slug: &str) -> bool {
        self.0.exists(slug)
    }
}

#[test]
fn dequeue_reports_a_teardown_that_left_the_worktree() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let stuck = Box::new(StuckWorktrees(MockWorktreeManager::new()));
    let mut pool = SessionPool::new(1, stuck, tx);
    let parent = running(&mut pool, 1560);
    walk(&mut pool, parent, &RETRYING);
    let child = for_issue(1560);
    let child_id = child.id;
    pool.enqueue(child);
    assert!(pool.finalize(parent), "the parent hands its worktree over");

    let outcome = pool.dequeue_as_shown(child_id, true);

    assert_eq!(outcome, Some(Dequeued::RemoveFailed), "never a silent Removed");
}
