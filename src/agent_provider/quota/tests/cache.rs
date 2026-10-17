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
