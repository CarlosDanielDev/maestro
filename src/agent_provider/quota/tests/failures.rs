use super::*;

#[tokio::test]
async fn failure_streak_warns_once_then_debugs_until_a_success() {
    let h = Harness::new();
    for _ in 0..5 {
        h.queue(status(500));
        assert!(h.fetcher.windows("claude").await.is_none());
        h.advance(Duration::seconds(61));
    }
    assert_eq!(h.fetcher.failure_counts(), (1, 4), "one warn, then debug");

    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("recovered");
    h.advance(Duration::seconds(61));

    h.queue(status(500));
    h.fetcher.windows("claude").await;
    assert_eq!(
        h.fetcher.failure_counts(),
        (2, 4),
        "a success ends the streak, so the next failure warns again"
    );
}
