//! Live quota wiring (issue #1065). A background task refreshes provider
//! windows off the UI thread. The event loop drains the result, feeds the pool
//! routing, raises the 97% alert once per crossing, and keeps badge text.
//!
//! Off by default: `[quota] enabled = false` spawns nothing and makes no request.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::App;
use crate::agent_provider::minimax::quota::{Clock, SystemClock};
use crate::agent_provider::quota::alert::QuotaAlerts;
use crate::agent_provider::quota::badge;
use crate::agent_provider::quota::refresh_interval;
use crate::agent_provider::quota::routing::collect_windows;
use crate::agent_provider::quota::types::ProviderWindows;
use crate::agent_provider::quota::{QuotaFetcher, alert::sanitize_provider_id};
use crate::notifications::types::InterruptLevel;
use crate::tui::activity_log::LogLevel;

pub type Snapshot = HashMap<String, ProviderWindows>;

pub struct QuotaRefresh {
    pub(super) enabled: bool,
    rx: Option<mpsc::UnboundedReceiver<Snapshot>>,
    windows: Snapshot,
    alerts: QuotaAlerts,
    clock: Box<dyn Clock>,
}

impl QuotaRefresh {
    pub fn new(clock: Box<dyn Clock>) -> Self {
        Self {
            enabled: false,
            rx: None,
            windows: HashMap::new(),
            alerts: QuotaAlerts::default(),
            clock,
        }
    }
}

impl Default for QuotaRefresh {
    fn default() -> Self {
        Self::new(Box::new(SystemClock))
    }
}

/// Start the refresh loop only when `enabled`. Off returns `None` before any
/// fetch, so no credential read and no HTTP request happen.
pub fn spawn_refresh(
    enabled: bool,
    fetcher: &Arc<QuotaFetcher>,
    provider_ids: Vec<String>,
    tx: mpsc::UnboundedSender<Snapshot>,
) -> Option<JoinHandle<()>> {
    if !enabled {
        return None;
    }
    let fetcher = Arc::clone(fetcher);
    Some(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(refresh_interval());
        let mut logged_empty = false;
        loop {
            ticker.tick().await;
            let ids: Vec<&str> = provider_ids.iter().map(String::as_str).collect();
            let snapshot = collect_windows(true, &fetcher, &ids).await;
            // Log once per no-data streak. Data arriving again ends the streak.
            if snapshot.is_empty() {
                if !logged_empty {
                    tracing::info!("no quota windows available; badges show no data");
                }
                logged_empty = true;
            } else {
                logged_empty = false;
            }
            if tx.send(snapshot).is_err() {
                break;
            }
        }
    }))
}

/// Home and cache paths for the real Claude quota source. `None` when the OS
/// dirs or the HTTP client are unavailable. Logs once, then the feature stays off.
fn production_fetcher() -> Option<QuotaFetcher> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    let cache = directories::ProjectDirs::from("io", "maestro", "maestro")?
        .cache_dir()
        .join("quota");
    match QuotaFetcher::system(home, cache) {
        Ok(fetcher) => Some(fetcher),
        Err(err) => {
            tracing::warn!(error = %err, "quota client unavailable; routing stays off");
            None
        }
    }
}

impl App {
    /// Called once when the event loop starts. Does nothing when quota is off.
    pub fn start_quota_refresh(&mut self) {
        if !self.quota.enabled || self.quota.rx.is_some() {
            return;
        }
        let Some(fetcher) = production_fetcher() else {
            return;
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let ids = self.pool.quota_provider_ids();
        spawn_refresh(true, &Arc::new(fetcher), ids, tx);
        self.quota.rx = Some(rx);
    }

    /// Drain pending snapshots. Runs each event-loop turn. Never fetches.
    pub fn apply_quota_updates(&mut self) {
        let Some(rx) = self.quota.rx.as_mut() else {
            return;
        };
        let mut snapshots = Vec::new();
        while let Ok(snapshot) = rx.try_recv() {
            snapshots.push(snapshot);
        }
        let Some(latest) = snapshots.last().cloned() else {
            return;
        };
        for snapshot in &snapshots {
            for (provider, pct) in self.quota.alerts.observe(snapshot) {
                let message = format!("{provider} at {pct:.0}% of a usage window");
                self.activity_log
                    .push_simple("Quota".into(), message.clone(), LogLevel::Warn);
                // Critical: only Critical shows a TUI banner and a desktop alert.
                self.notifications
                    .notify(InterruptLevel::Critical, "Quota alert", &message);
            }
        }
        self.pool
            .set_quota_routing(latest.clone(), Box::new(SystemClock));
        self.quota.windows = latest;
    }

    /// One badge per configured provider, from windows already in memory.
    /// Empty when quota is off, so the default dashboard does not change.
    pub fn quota_badges(&self) -> Vec<String> {
        if !self.quota.enabled {
            return Vec::new();
        }
        let now = self.quota.clock.now();
        self.pool
            .quota_provider_ids()
            .iter()
            .map(|id| badge::badge(&sanitize_provider_id(id), self.quota.windows.get(id), now))
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
}
