//! Unit tests for `quota_refresh.rs`. Extracted from the module body
//! via `#[cfg(test)] #[path = "quota_refresh_tests.rs"] mod tests;` to
//! honor the 400-line file cap.

use super::*;
use crate::agent_provider::AgentProvider;
use crate::agent_provider::quota::fetch::{HttpResponse, QuotaHttp};
use crate::agent_provider::quota::types::{QuotaWindow, TransportError};
use crate::agent_provider::test_fakes::FakeHttpProvider;
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap()
}

#[derive(Debug)]
struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

/// Counts HTTP calls and always answers one on-track five-hour window.
#[derive(Clone, Default)]
struct CountingHttp(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl QuotaHttp for CountingHttp {
    async fn get_usage(&self, _token: &str) -> Result<HttpResponse, TransportError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(HttpResponse {
            status: 200,
            body: r#"{"five_hour":{"utilization":40.0,"resets_at":"2026-10-17T14:30:00Z"}}"#
                .to_string(),
        })
    }
}

/// Home with a credential file, so a read would succeed if it happened.
fn fetcher_with(calls: &CountingHttp) -> (tempfile::TempDir, tempfile::TempDir, QuotaFetcher) {
    let home = tempfile::tempdir().expect("home dir");
    let cache = tempfile::tempdir().expect("cache dir");
    let creds_dir = home.path().join(".claude");
    std::fs::create_dir_all(&creds_dir).expect("claude dir");
    std::fs::write(
        creds_dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"sk-test-fake"}}"#,
    )
    .expect("write creds");
    let fetcher = QuotaFetcher::new(
        home.path().to_path_buf(),
        cache.path().to_path_buf(),
        Box::new(calls.clone()),
        Box::new(FixedClock(now())),
    );
    (home, cache, fetcher)
}

#[tokio::test]
async fn quota_disabled_spawns_nothing() {
    let calls = CountingHttp::default();
    let (_home, _cache, fetcher) = fetcher_with(&calls);
    let (tx, _rx) = mpsc::unbounded_channel();

    let handle = spawn_refresh(false, &Arc::new(fetcher), vec!["claude".into()], tx);

    assert!(handle.is_none(), "off must not start a task");
    tokio::task::yield_now().await;
    assert_eq!(calls.0.load(Ordering::SeqCst), 0, "off must send no HTTP");
}

#[tokio::test]
async fn enabled_refresh_sends_claude_windows() {
    // Control for the test above: the same wiring does fetch when on.
    let calls = CountingHttp::default();
    let (_home, _cache, fetcher) = fetcher_with(&calls);
    let (tx, mut rx) = mpsc::unbounded_channel();

    let handle = spawn_refresh(true, &Arc::new(fetcher), vec!["claude".into()], tx)
        .expect("on must start a task");
    let snapshot = rx.recv().await.expect("first snapshot");
    handle.abort();

    assert!(snapshot.contains_key("claude"));
    assert_eq!(calls.0.load(Ordering::SeqCst), 1);
}

fn snapshot(util: f64) -> Snapshot {
    HashMap::from([(
        "claude".to_string(),
        ProviderWindows {
            five_hour: Some(QuotaWindow {
                utilization_pct: util,
                resets_at: now() + ChronoDuration::minutes(150),
            }),
            seven_day: None,
            seven_day_sonnet: None,
            stale: false,
        },
    )])
}

fn app_with_feed() -> (App, mpsc::UnboundedSender<Snapshot>) {
    let mut app = crate::tui::make_test_app("maestro-quota-refresh-test");
    let (tx, rx) = mpsc::unbounded_channel();
    app.quota.rx = Some(rx);
    (app, tx)
}

fn quota_alert_count(app: &App) -> usize {
    app.notifications
        .active_banners()
        .iter()
        .filter(|n| n.title == "Quota alert")
        .count()
}

fn feed(app: &mut App, tx: &mpsc::UnboundedSender<Snapshot>, util: f64) {
    tx.send(snapshot(util)).expect("feed open");
    app.apply_quota_updates();
}

#[test]
fn alert_notifies_once_per_crossing_end_to_end() {
    let (mut app, tx) = app_with_feed();

    feed(&mut app, &tx, 97.0);
    assert_eq!(quota_alert_count(&app), 1, "crossing 97 fires once");
    feed(&mut app, &tx, 99.0);
    feed(&mut app, &tx, 95.0);
    assert_eq!(quota_alert_count(&app), 1, "staying high does not repeat");
    feed(&mut app, &tx, 89.0);
    feed(&mut app, &tx, 97.0);
    assert_eq!(quota_alert_count(&app), 2, "re-armed below 90, one more");
}

#[test]
fn applied_snapshot_feeds_pool_routing() {
    // claude is critical (90% at 50% elapsed), qwen is on track.
    let (mut app, tx) = app_with_feed();
    app.pool.set_agent_providers(HashMap::from([(
        "qwen".to_string(),
        Arc::new(FakeHttpProvider) as Arc<dyn AgentProvider>,
    )]));
    let mut hot = snapshot(90.0);
    hot.insert("qwen".to_string(), snapshot(40.0)["claude"].clone());
    tx.send(hot).expect("feed open");

    app.apply_quota_updates();

    assert_eq!(
        app.pool.quota_routed_provider().map(|p| p.id()),
        Some("qwen")
    );
}

#[test]
fn badges_show_no_data_until_a_snapshot_arrives() {
    let mut app = crate::tui::make_test_app("maestro-quota-refresh-test");
    app.quota = QuotaRefresh::new(Box::new(FixedClock(now())));
    app.quota.enabled = true;
    assert_eq!(app.quota_badges(), vec!["claude no data".to_string()]);

    let (tx, rx) = mpsc::unbounded_channel();
    app.quota.rx = Some(rx);
    feed(&mut app, &tx, 40.0);

    assert_eq!(app.quota_badges(), vec!["claude 40% ↻2h30m →".to_string()]);
}

#[test]
fn badge_map_keys_by_raw_provider_id_with_pacing_glyph() {
    let mut app = crate::tui::make_test_app("maestro-quota-refresh-test");
    app.quota = QuotaRefresh::new(Box::new(FixedClock(now())));
    app.quota.enabled = true;

    let (tx, rx) = mpsc::unbounded_channel();
    app.quota.rx = Some(rx);
    feed(&mut app, &tx, 90.0); // critical: 90% used at 50% elapsed

    let map = app.quota_badge_map();
    assert_eq!(
        map.get("claude").map(String::as_str),
        Some("claude 90% ↻2h30m ↓"),
        "keys stay the raw agent_id; the text renders the sanitized id"
    );
}

#[test]
fn badge_map_is_empty_when_quota_is_off() {
    let app = crate::tui::make_test_app("maestro-quota-refresh-test");
    assert!(app.quota_badge_map().is_empty());
}

#[test]
fn explicit_provider_wins_over_quota_routing() {
    // Routing would pick qwen (claude is critical), but the session names
    // claude. The explicit choice must stay.
    let (mut app, tx) = app_with_feed();
    app.pool.set_agent_providers(HashMap::from([(
        "qwen".to_string(),
        Arc::new(FakeHttpProvider) as Arc<dyn AgentProvider>,
    )]));
    let mut hot = snapshot(90.0);
    hot.insert("qwen".to_string(), snapshot(40.0)["claude"].clone());
    tx.send(hot).expect("feed open");
    app.apply_quota_updates();
    let pinned = crate::session::types::Session::new(
        "pinned".into(),
        "model".into(),
        "default".into(),
        None,
        None,
    )
    .with_agent_id(Some("claude".to_string()));
    app.pool.enqueue(pinned);

    let ids = app.pool.try_promote();
    let managed = app.pool.get_active_mut(ids[0]).expect("promoted");

    assert_eq!(managed.provider_id(), "claude");
}

#[test]
fn badges_are_hidden_when_quota_is_off() {
    // Default-off: the dashboard must not gain "no data" rows for everyone.
    let (app, _tx) = app_with_feed();
    assert!(!app.quota.enabled);
    assert!(app.quota_badges().is_empty());
}
