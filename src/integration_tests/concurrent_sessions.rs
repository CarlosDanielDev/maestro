use crate::integration_tests::helpers::*;
use crate::session::types::Session;

#[test]
fn max_one_allows_only_one_active() {
    let mut pool = make_pool(1);
    pool.enqueue(make_session("s1"));
    pool.enqueue(make_session("s2"));
    pool.enqueue(make_session("s3"));

    let promoted = pool.try_promote();

    assert_eq!(promoted.len(), 1);
    assert_eq!(pool.active_count(), 1);
    assert_eq!(pool.queued_count(), 2);
}

#[test]
fn max_two_promotes_two_sessions() {
    let mut pool = make_pool(2);
    for i in 0..4 {
        pool.enqueue(make_session(&format!("s{}", i)));
    }

    let promoted = pool.try_promote();

    assert_eq!(promoted.len(), 2);
    assert_eq!(pool.active_count(), 2);
    assert_eq!(pool.queued_count(), 2);
}

#[test]
fn completing_one_session_frees_slot_for_next() {
    let mut pool = make_pool(1);
    let s1 = make_session("first");
    let id1 = s1.id;
    pool.enqueue(s1);
    pool.enqueue(make_session("second"));
    pool.try_promote(); // promotes first only

    assert_eq!(pool.active_count(), 1);
    assert_eq!(pool.queued_count(), 1);

    pool.on_session_completed(id1);
    let promoted = pool.try_promote();

    assert_eq!(
        promoted.len(),
        1,
        "completing first must allow second to be promoted"
    );
    assert_eq!(pool.active_count(), 1);
    assert_eq!(pool.queued_count(), 0);
}

#[test]
fn promote_at_capacity_returns_empty() {
    let mut pool = make_pool(2);
    pool.enqueue(make_session("a"));
    pool.enqueue(make_session("b"));
    pool.try_promote(); // fills capacity

    pool.enqueue(make_session("c"));
    let promoted = pool.try_promote();

    assert_eq!(
        promoted.len(),
        0,
        "pool at capacity must not promote additional sessions"
    );
    assert_eq!(pool.active_count(), 2);
    assert_eq!(pool.queued_count(), 1);
}

#[test]
fn fifo_ordering_preserved_under_promotion() {
    let mut pool = make_pool(3);

    let sessions: Vec<Session> = (0..5)
        .map(|i| {
            Session::new(
                format!("task-{}", i),
                "opus".to_string(),
                "orchestrator".to_string(),
                Some(100 + i as u64),
            )
        })
        .collect();
    let expected_first_three: Vec<u64> = sessions[..3]
        .iter()
        .map(|s| s.issue_number.unwrap())
        .collect();

    for s in sessions {
        pool.enqueue(s);
    }

    let promoted_ids = pool.try_promote();

    // Collect issue numbers of promoted sessions
    let promoted_issues: Vec<u64> = promoted_ids
        .iter()
        .filter_map(|id| {
            pool.all_sessions()
                .iter()
                .find(|s| s.id == *id)
                .and_then(|s| s.issue_number)
        })
        .collect();

    for issue in &expected_first_three {
        assert!(
            promoted_issues.contains(issue),
            "issue #{} must be in the first promoted batch",
            issue
        );
    }
}

#[test]
fn multiple_promotions_saturate_and_drain() {
    let mut pool = make_pool(2);
    let sessions: Vec<Session> = (0..6).map(|i| make_session(&format!("t{}", i))).collect();
    let ids: Vec<_> = sessions.iter().map(|s| s.id).collect();
    for s in sessions {
        pool.enqueue(s);
    }

    // Round 1
    let p1 = pool.try_promote();
    assert_eq!(p1.len(), 2);

    pool.on_session_completed(ids[0]);
    pool.on_session_completed(ids[1]);

    // Round 2
    let p2 = pool.try_promote();
    assert_eq!(p2.len(), 2);

    pool.on_session_completed(ids[2]);
    pool.on_session_completed(ids[3]);

    // Round 3
    let p3 = pool.try_promote();
    assert_eq!(p3.len(), 2);

    pool.on_session_completed(ids[4]);
    pool.on_session_completed(ids[5]);

    assert!(pool.all_done());
    assert_eq!(pool.total_count(), 6);
}
