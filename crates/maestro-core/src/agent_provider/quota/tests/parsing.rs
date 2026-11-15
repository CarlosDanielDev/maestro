use super::*;

#[tokio::test]
async fn parses_claude_windows_from_fixture() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    let five = windows.five_hour.expect("five_hour");
    assert_eq!(five.utilization_pct, 37.5);
    assert_eq!(five.resets_at, at(16, 0, 0));
    let week = windows.seven_day.expect("seven_day");
    assert_eq!(week.utilization_pct, 12.0);
    let sonnet = windows.seven_day_sonnet.expect("seven_day_sonnet");
    assert_eq!(sonnet.utilization_pct, 4.0);
    assert!(!windows.stale);
}

#[tokio::test]
async fn missing_window_is_none_not_zero() {
    let h = Harness::new();
    h.queue(ok(
        r#"{"five_hour":{"utilization":10,"resets_at":"2026-10-17T16:00:00Z"}}"#,
    ));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(windows.five_hour.is_some());
    assert!(windows.seven_day.is_none());
    assert!(windows.seven_day_sonnet.is_none());
}

#[tokio::test]
async fn window_without_reset_time_is_dropped_not_zero() {
    let h = Harness::new();
    h.queue(ok(
        r#"{"five_hour":{"utilization":10},"seven_day":{"utilization":5,"resets_at":"2026-10-23T09:00:00Z"}}"#,
    ));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(windows.five_hour.is_none());
    assert_eq!(windows.seven_day.expect("seven_day").utilization_pct, 5.0);
}

#[tokio::test]
async fn malformed_reset_time_is_dropped_not_zero() {
    let h = Harness::new();
    h.queue(ok(
        r#"{"five_hour":{"utilization":10,"resets_at":"not-a-date"},"seven_day":{"utilization":5,"resets_at":"2026-10-23T09:00:00Z"}}"#,
    ));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(windows.five_hour.is_none());
    assert_eq!(windows.seven_day.expect("seven_day").utilization_pct, 5.0);
}

#[tokio::test]
async fn out_of_range_utilization_is_dropped_not_zero() {
    let h = Harness::new();
    h.queue(ok(r#"{
            "five_hour":{"utilization":-5,"resets_at":"2026-10-17T16:00:00Z"},
            "seven_day":{"utilization":150,"resets_at":"2026-10-23T09:00:00Z"},
            "seven_day_sonnet":{"utilization":100,"resets_at":"2026-10-23T09:00:00Z"}
        }"#));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(windows.five_hour.is_none(), "negative is rejected");
    assert!(windows.seven_day.is_none(), "above 100 is rejected");
    assert_eq!(
        windows.seven_day_sonnet.expect("sonnet").utilization_pct,
        100.0,
        "100 is the inclusive upper bound"
    );
}

#[tokio::test]
async fn response_without_any_window_is_no_data() {
    let h = Harness::new();
    h.queue(ok(r#"{"unrelated":true}"#));

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 1);
}
