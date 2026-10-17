use super::*;

use super::super::cache::{self as store, LockOutcome};

#[tokio::test]
async fn fresh_cache_is_served_without_second_http_call() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(30));
    let windows = h.fetcher.windows("claude").await.expect("cached");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn expired_ttl_refetches() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(ok(
        r#"{"five_hour":{"utilization":80,"resets_at":"2026-10-17T16:00:00Z"}}"#,
    ));
    let windows = h.fetcher.windows("claude").await.expect("refetch");

    assert_eq!(h.calls(), 2);
    assert_eq!(windows.five_hour.expect("five_hour").utilization_pct, 80.0);
    assert!(!windows.stale);
}

#[tokio::test]
async fn clock_moved_backwards_makes_cache_expired_not_fresh() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    // fetched_at is now in the future. It must not count as fresh.
    h.advance(Duration::seconds(-30));
    h.queue(ok(FIXTURE));
    let windows = h.fetcher.windows("claude").await.expect("refetch");

    assert_eq!(h.calls(), 2, "future fetched_at must trigger a fetch");
    assert!(!windows.stale);
}

#[tokio::test]
async fn backoff_further_out_than_five_minutes_is_clamped() {
    let h = Harness::new();
    h.seed(&store::CacheEntry {
        fetched_at: Some(at(11, 0, 0)),
        windows: Some(sample_windows()),
        backoff_until: Some(at(12, 0, 0) + Duration::days(1)),
        attempted_at: Some(at(11, 59, 0)),
    });

    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");
    assert!(windows.stale);
    assert_eq!(
        h.calls(),
        0,
        "backoff is active right after the clamp starts"
    );

    h.advance(Duration::minutes(5) + Duration::seconds(1));
    h.queue(ok(FIXTURE));
    let windows = h.fetcher.windows("claude").await.expect("polled again");

    assert!(!windows.stale);
    assert_eq!(
        h.calls(),
        1,
        "a corrupt one-day backoff must not outlast five minutes"
    );
}

#[tokio::test]
async fn rate_limit_429_keeps_last_good_snapshot_flagged_stale() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(status(429));
    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");

    assert!(windows.stale);
    assert_eq!(windows.five_hour.expect("five_hour").utilization_pct, 37.5);
    assert_eq!(h.calls(), 2);
}

#[tokio::test]
async fn rate_limited_provider_is_not_polled_for_five_minutes() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");
    h.advance(Duration::seconds(61));
    h.queue(status(429));
    h.fetcher.windows("claude").await.expect("stale after 429");

    h.advance(Duration::minutes(4));
    let windows = h.fetcher.windows("claude").await.expect("still backed off");
    assert!(windows.stale);
    assert_eq!(h.calls(), 2, "no poll during the 5 minute backoff");

    h.advance(Duration::minutes(2));
    h.queue(ok(FIXTURE));
    let windows = h.fetcher.windows("claude").await.expect("polled again");
    assert!(!windows.stale);
    assert_eq!(h.calls(), 3);
}

#[tokio::test]
async fn rate_limit_without_cache_is_no_data() {
    let h = Harness::new();
    h.queue(status(429));

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn transport_failure_serves_last_good_snapshot_as_stale() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(Err(TransportError("connection reset".to_string())));
    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");

    assert!(windows.stale);
}

/// After a failed attempt, the next call inside TTL must not hit the API again.
async fn assert_failure_not_retried_within_ttl(failure: Result<HttpResponse, TransportError>) {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(failure);
    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");
    assert!(windows.stale);
    assert_eq!(h.calls(), 2);

    h.advance(Duration::seconds(30));
    let windows = h.fetcher.windows("claude").await.expect("stale inside TTL");
    assert!(windows.stale);
    assert_eq!(h.calls(), 2, "no request inside TTL of the failed attempt");

    h.advance(Duration::seconds(31));
    h.queue(ok(FIXTURE));
    let windows = h
        .fetcher
        .windows("claude")
        .await
        .expect("retried after TTL");
    assert!(!windows.stale);
    assert_eq!(h.calls(), 3);
}

#[tokio::test]
async fn server_error_is_not_retried_within_ttl() {
    assert_failure_not_retried_within_ttl(status(500)).await;
}

#[tokio::test]
async fn unauthorized_is_not_retried_within_ttl() {
    assert_failure_not_retried_within_ttl(status(401)).await;
}

#[tokio::test]
async fn failed_attempt_without_cache_is_not_retried_within_ttl() {
    let h = Harness::new();
    h.queue(status(500));

    assert!(h.fetcher.windows("claude").await.is_none());
    h.advance(Duration::seconds(30));
    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn unparseable_200_is_not_retried_within_ttl() {
    let h = Harness::new();
    h.queue(ok(r#"{"unrelated":true}"#));

    assert!(h.fetcher.windows("claude").await.is_none());
    h.advance(Duration::seconds(30));
    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn locked_cache_is_read_without_polling() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");
    h.advance(Duration::seconds(61));

    // Another process holds the lock and is fetching.
    let _held = match store::try_lock(&h.lock_path()).expect("lock file") {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::Busy => panic!("lock should be free in test setup"),
    };
    let windows = h
        .fetcher
        .windows("claude")
        .await
        .expect("cached while locked");

    assert!(windows.stale);
    assert_eq!(h.calls(), 1, "no poll while another process holds the lock");
}

#[tokio::test]
async fn locked_without_cache_is_no_data() {
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).expect("cache dir");
    let _held = match store::try_lock(&h.lock_path()).expect("lock file") {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::Busy => panic!("lock should be free in test setup"),
    };

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 0);
}

#[test]
fn lock_is_released_when_guard_drops() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.quota.lock");

    let first = match store::try_lock(&path).expect("first lock") {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::Busy => panic!("first acquire should succeed"),
    };
    assert!(matches!(
        store::try_lock(&path).expect("second lock"),
        LockOutcome::Busy
    ));

    drop(first);

    assert!(matches!(
        store::try_lock(&path).expect("third lock"),
        LockOutcome::Acquired(_)
    ));
}

#[tokio::test]
async fn unlocked_fetch_does_not_persist_cache() {
    let h = Harness::new();
    // A directory at the lock path makes the lock open fail, so the fetch runs unlocked.
    std::fs::create_dir_all(h.lock_path()).expect("lock path as dir");
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
    assert!(
        !h.cache_path().exists(),
        "unlocked fetch must not write the cache"
    );
}

#[tokio::test]
async fn corrupt_cache_file_is_treated_as_empty() {
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).expect("cache dir");
    std::fs::write(h.cache_path(), "{not json").expect("write junk");
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("fetched");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn truncated_cache_file_is_treated_as_miss() {
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).expect("cache dir");
    std::fs::write(h.cache_path(), r#"{"fetched_at":"2026-10-17T11:5"#).expect("write");
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("fetched");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn non_utf8_cache_file_is_treated_as_miss() {
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).expect("cache dir");
    std::fs::write(h.cache_path(), [0xff, 0xfe, 0x00, b'{', 0x80]).expect("write bytes");
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("fetched");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn future_attempt_owning_a_429_backoff_is_absent() {
    let h = Harness::new();
    h.seed(&store::CacheEntry {
        fetched_at: None,
        windows: None,
        backoff_until: Some(at(12, 5, 0)),
        attempted_at: Some(at(12, 0, 0)),
    });
    // The clock is now behind the attempt that set the backoff.
    h.advance(Duration::hours(-1));
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("fetched");

    assert!(!windows.stale);
    assert_eq!(
        h.calls(),
        1,
        "a future attempt owns no backoff, so a fetch is allowed"
    );
}

#[tokio::test]
async fn future_failed_attempt_is_absent_so_fetch_happens() {
    let h = Harness::new();
    h.seed(&store::CacheEntry {
        fetched_at: Some(at(10, 0, 0)),
        windows: Some(sample_windows()),
        backoff_until: None,
        attempted_at: Some(at(12, 0, 0)),
    });
    h.advance(Duration::hours(-1));
    h.queue(status(500));

    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");
    assert!(windows.stale);
    assert_eq!(
        h.calls(),
        1,
        "a future failed attempt does not block a fetch"
    );

    h.advance(Duration::seconds(30));
    h.fetcher.windows("claude").await.expect("stale inside TTL");
    assert_eq!(h.calls(), 1, "the attempt just made now gates the retry");
}

#[tokio::test]
async fn failed_rename_leaves_no_temp_file() {
    let h = Harness::new();
    // A directory at the cache path makes the rename fail. Readers treat it as a miss.
    std::fs::create_dir_all(h.cache_path()).expect("dir at cache path");
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("fetched");

    assert!(!windows.stale);
    let leftovers: Vec<String> = std::fs::read_dir(&h.cache_dir)
        .expect("read cache dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}
