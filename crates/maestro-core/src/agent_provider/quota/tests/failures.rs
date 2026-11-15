use super::*;

use super::super::cache::LockOutcome;
use super::super::failures::{FailureSink, Level};

/// Records every report as (level, key), so a test can see what was emitted.
#[derive(Debug, Clone, Default)]
struct RecordingSink(Arc<Mutex<Vec<(Level, &'static str)>>>);

impl FailureSink for RecordingSink {
    fn report(&self, level: Level, key: &'static str, _what: &str) {
        self.0.lock().expect("sink lock").push((level, key));
    }
}

impl RecordingSink {
    fn levels(&self, key: &str) -> Vec<Level> {
        self.0
            .lock()
            .expect("sink lock")
            .iter()
            .filter(|(_, k)| *k == key)
            .map(|(level, _)| *level)
            .collect()
    }
}

use Level::{Debug as D, Warn as W};

#[tokio::test]
async fn failure_streak_warns_once_then_debugs_until_a_success() {
    let sink = RecordingSink::default();
    let h = Harness::new().with_sink(Box::new(sink.clone()));
    for _ in 0..5 {
        h.queue(status(500));
        assert!(h.fetcher.windows("claude").await.is_none());
        h.advance(Duration::seconds(61));
    }
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("recovered");
    h.advance(Duration::seconds(61));
    for _ in 0..2 {
        h.queue(status(500));
        h.fetcher.windows("claude").await;
        h.advance(Duration::seconds(61));
    }

    assert_eq!(
        sink.levels("fetch"),
        vec![W, D, D, D, D, W, D],
        "1 warn, 4 debug, then a success, then 1 warn, 1 debug"
    );
}

#[tokio::test]
async fn fresh_cache_hit_ends_the_failure_streak() {
    let sink = RecordingSink::default();
    let h = Harness::new().with_sink(Box::new(sink.clone()));
    h.queue(status(500));
    h.fetcher.windows("claude").await;
    // Another process wrote a fresh snapshot. This call is served from cache.
    h.seed(&cache_mod::CacheEntry {
        fetched_at: Some(at(12, 0, 0)),
        windows: Some(sample_windows()),
        backoff_until: None,
        attempted_at: None,
    });
    assert!(h.fetcher.windows("claude").await.is_some());

    h.advance(Duration::seconds(61));
    h.queue(status(500));
    h.fetcher.windows("claude").await;

    assert_eq!(
        sink.levels("fetch"),
        vec![W, W],
        "the cache hit ended the streak"
    );
}

#[tokio::test]
async fn busy_lock_with_fresh_cache_ends_the_failure_streak() {
    let sink = RecordingSink::default();
    let h = Harness::new().with_sink(Box::new(sink.clone()));
    h.queue(status(500));
    h.fetcher.windows("claude").await;
    h.advance(Duration::seconds(61));
    h.seed(&cache_mod::CacheEntry {
        fetched_at: Some(at(12, 1, 1)),
        windows: Some(sample_windows()),
        backoff_until: None,
        attempted_at: None,
    });
    // Another process holds the lock and serves its fresh snapshot.
    let _held = match cache_mod::try_lock(&h.lock_path()).expect("lock file") {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::Busy => panic!("lock should be free in test setup"),
    };
    assert!(h.fetcher.windows("claude").await.is_some());
    drop(_held);

    h.advance(Duration::seconds(61));
    h.queue(status(500));
    h.fetcher.windows("claude").await;

    assert_eq!(
        sink.levels("fetch"),
        vec![W, W],
        "the busy-path read ended the streak"
    );
}

#[tokio::test]
async fn unwritable_cache_dir_warns_once_across_ticks() {
    let sink = RecordingSink::default();
    let h = Harness::new();
    // The cache dir's parent is a plain file, so every create_dir_all call fails.
    let blocker = h.cache_dir.with_file_name("blocker");
    std::fs::write(&blocker, "not a dir").expect("file as parent");
    let h = Harness {
        cache_dir: blocker.join("cache"),
        ..h
    }
    .with_sink(Box::new(sink.clone()));
    for _ in 0..4 {
        h.queue(status(500));
        h.fetcher.windows("claude").await;
        h.advance(Duration::seconds(61));
    }

    assert_eq!(sink.levels("cache dir"), vec![W, D, D, D]);
}
