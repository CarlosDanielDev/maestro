use crate::integration_tests::helpers::*;
use crate::session::manager::ManagedSession;
use crate::session::types::{SessionStatus, StreamEvent};
use crate::session::worktree::MockWorktreeManager;

#[test]
fn session_starts_queued_after_new() {
    let session = make_session("fix the bug");
    assert_eq!(session.status, SessionStatus::Queued);
    assert!(session.started_at.is_none());
    assert!(session.finished_at.is_none());
    assert_eq!(session.cost_usd, 0.0);
    assert!(session.activity_log.is_empty());
}

#[test]
fn enqueue_increments_queued_count() {
    let mut pool = make_pool(2);
    pool.enqueue(make_session("s1"));
    pool.enqueue(make_session("s2"));
    assert_eq!(pool.queued_count(), 2);
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn promote_moves_session_to_active_slot() {
    let mut pool = make_pool(2);
    let s = make_session("work");
    let id = s.id;
    pool.enqueue(s);

    let promoted = pool.try_promote();

    assert_eq!(promoted, vec![id]);
    assert_eq!(pool.active_count(), 1);
    assert_eq!(pool.queued_count(), 0);
}

#[test]
fn promote_creates_worktree_for_issue_session() {
    let wt = MockWorktreeManager::new();
    let mut pool = make_pool_with_worktree(2, wt);
    pool.enqueue(make_session_with_issue(42));

    let promoted = pool.try_promote();
    let id = promoted[0];

    let managed = pool.get_active_mut(id).unwrap();
    assert!(managed.worktree_path.is_some());
    let path = managed.worktree_path.as_ref().unwrap().to_string_lossy();
    assert!(
        path.contains("issue-42"),
        "worktree path must contain the slug"
    );
    assert!(managed.branch_name.is_some());
    assert!(
        managed.branch_name.as_ref().unwrap().contains("issue-42"),
        "branch name must contain the slug"
    );
}

#[test]
fn promote_uses_session_id_slug_when_no_issue_number() {
    let wt = MockWorktreeManager::new();
    let mut pool = make_pool_with_worktree(2, wt);
    pool.enqueue(make_session("no issue"));

    let promoted = pool.try_promote();
    let id = promoted[0];

    let managed = pool.get_active_mut(id).unwrap();
    let path = managed.worktree_path.as_ref().unwrap().to_string_lossy();
    assert!(
        path.contains("session-"),
        "slug must start with 'session-' for prompt-only sessions"
    );
}

#[test]
fn on_completed_moves_from_active_to_finished() {
    let mut pool = make_pool(2);
    let s = make_session("task");
    let id = s.id;
    pool.enqueue(s);
    pool.try_promote();
    assert_eq!(pool.active_count(), 1);

    pool.finalize_and_teardown(id);

    assert_eq!(pool.active_count(), 0);
    assert_eq!(pool.total_count(), 1); // 1 in finished
    assert_eq!(pool.queued_count(), 0);
}

#[test]
fn completed_session_cleans_up_worktree() {
    let wt = MockWorktreeManager::new();
    let mut pool = make_pool_with_worktree(1, wt);
    let s = make_session_with_issue(10);
    let id = s.id;
    pool.enqueue(s);
    pool.try_promote();

    pool.finalize_and_teardown(id);

    // Re-enqueueing same issue slug succeeds (proves worktree was cleaned up)
    let s2 = make_session_with_issue(10);
    pool.enqueue(s2);
    let promoted = pool.try_promote();
    assert_eq!(promoted.len(), 1, "re-promoting after cleanup must succeed");
}

#[test]
fn all_done_only_true_after_every_session_finishes() {
    let mut pool = make_pool(3);
    let s1 = make_session("a");
    let s2 = make_session("b");
    let id1 = s1.id;
    let id2 = s2.id;
    pool.enqueue(s1);
    pool.enqueue(s2);
    pool.try_promote();

    assert!(!pool.all_done());

    pool.finalize_and_teardown(id1);
    assert!(!pool.all_done());

    pool.finalize_and_teardown(id2);
    assert!(pool.all_done());
}

#[test]
fn handle_event_completed_transitions_session_status() {
    let session = make_running_session("s");
    let mut managed = ManagedSession::new(session);

    managed.handle_event(&StreamEvent::Completed { cost_usd: 1.23 });

    assert_eq!(managed.session.status, SessionStatus::Completed);
    assert!(managed.session.finished_at.is_some());
    assert_eq!(managed.session.current_activity, "Done");
    assert!((managed.session.cost_usd - 1.23).abs() < f64::EPSILON);
}

#[test]
fn handle_event_error_transitions_session_status() {
    let session = make_running_session("s");
    let mut managed = ManagedSession::new(session);

    managed.handle_event(&StreamEvent::Error {
        message: "rate limit exceeded".to_string(),
    });

    assert_eq!(managed.session.status, SessionStatus::Errored);
    // Errored is now recoverable (can transition to Retrying), so finished_at is not set
    assert_eq!(managed.session.current_activity, "Error");
    assert!(
        managed
            .session
            .activity_log
            .iter()
            .any(|e| e.message.contains("rate limit exceeded"))
    );
}

#[test]
fn completed_does_not_re_transition_terminal_session() {
    let session = make_session("s");
    let mut managed = ManagedSession::new(session);

    // Use Killed (hard terminal) instead of Errored (now recoverable)
    managed.session.status = SessionStatus::Running;
    managed.session.status = SessionStatus::Killed;

    managed.handle_event(&StreamEvent::Completed { cost_usd: 0.5 });

    assert_eq!(
        managed.session.status,
        SessionStatus::Killed,
        "a terminal session must not be re-transitioned by a Completed event"
    );
}
