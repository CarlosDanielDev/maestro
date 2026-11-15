use super::*;

use super::super::cache as store;

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
async fn non_429_attempt_clears_an_old_backoff() {
    let h = Harness::new();
    // A 429 at 12:00 sets a backoff until 12:05.
    h.queue(status(429));
    assert!(h.fetcher.windows("claude").await.is_none());

    // The clock jumps back. The old backoff is now owned by an attempt in the future.
    h.advance(Duration::hours(-1));
    h.queue(status(500));
    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(
        h.calls(),
        2,
        "the 429 owner is in the future, so the 500 attempt goes out"
    );

    // Past the TTL gate of the 500 attempt, but before the old backoff's end.
    h.advance(Duration::minutes(2));
    h.queue(ok(FIXTURE));
    let windows = h.fetcher.windows("claude").await.expect("fetch allowed");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 3, "a non-429 attempt must clear the old backoff");
}
