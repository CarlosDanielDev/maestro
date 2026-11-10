//! Live quota wiring (issue #1065). A background task refreshes provider
//! windows off the UI thread. The event loop drains the result, feeds the pool
//! routing, raises the 97% alert once per crossing, and keeps badge text.
//!
//! Off by default: `[quota] enabled = false` spawns nothing and makes no request.

use std::collections::{BTreeMap, HashMap};
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

    /// Badge per provider id, keyed by the raw id a session's `agent_id`
    /// carries (Fleet board rows look up by it, #1065). Values render the
    /// sanitized id. Empty when quota is off.
    pub fn quota_badge_map(&self) -> BTreeMap<String, String> {
        if !self.quota.enabled {
            return BTreeMap::new();
        }
        let now = self.quota.clock.now();
        self.pool
            .quota_provider_ids()
            .into_iter()
            .map(|id| {
                let text =
                    badge::badge(&sanitize_provider_id(&id), self.quota.windows.get(&id), now);
                (id, text)
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "quota_refresh_tests.rs"]
mod tests;
