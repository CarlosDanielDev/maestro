use crate::integration_tests::helpers::*;
use crate::session::worktree::MockWorktreeManager;

#[test]
fn failure_is_non_fatal_session_still_promoted() {
    let wt = MockWorktreeManager::new();
    wt.set_create_error(true);
    let mut pool = make_pool_with_worktree(2, wt);
    pool.enqueue(make_session_with_issue(88));

    let promoted = pool.try_promote();

    assert_eq!(
        promoted.len(),
        1,
        "worktree failure must not prevent promotion"
    );
    assert_eq!(pool.active_count(), 1);

    let managed = pool.get_active_mut(promoted[0]).unwrap();
    assert!(
        managed.worktree_path.is_none(),
        "worktree_path must be None when creation failed"
    );
}

#[test]
fn failure_logs_activity_on_session() {
    let wt = MockWorktreeManager::new();
    wt.set_create_error(true);
    let mut pool = make_pool_with_worktree(2, wt);
    pool.enqueue(make_session_with_issue(99));

    pool.try_promote();

    let session = pool.all_sessions().into_iter().next().unwrap();
    assert!(
        session.activity_log.iter().any(|e| {
            let msg = e.message.to_lowercase();
            msg.contains("worktree")
                && (msg.contains("skip") || msg.contains("cwd") || msg.contains("fail"))
        }),
        "a worktree failure must be logged on the session activity log"
    );
}

#[test]
fn multiple_sessions_each_get_unique_worktree() {
    let wt = MockWorktreeManager::new();
    let mut pool = make_pool_with_worktree(3, wt);
    pool.enqueue(make_session_with_issue(1));
    pool.enqueue(make_session_with_issue(2));
    pool.enqueue(make_session_with_issue(3));

    pool.try_promote();
    assert_eq!(pool.active_count(), 3);

    let ids: Vec<_> = pool.all_sessions().iter().map(|s| s.id).collect();
    let mut paths: Vec<String> = Vec::new();
    for id in ids {
        if let Some(managed) = pool.get_active_mut(id) {
            if let Some(ref path) = managed.worktree_path {
                paths.push(path.to_string_lossy().to_string());
            }
        }
    }

    assert_eq!(paths.len(), 3, "all 3 sessions must have worktree paths");
    paths.sort();
    let unique_count = paths.len();
    paths.dedup();
    assert_eq!(
        paths.len(),
        unique_count,
        "all worktree paths must be distinct"
    );
}
