//! Provider quota windows (issue #1064).
//!
//! [`QuotaFetcher::windows`] returns the rate-limit windows (`five_hour`,
//! `seven_day`, ...) that a provider reports, so later slices can warn or
//! route before a session is cut off. Only Claude Code has a quota source in v1.
//!
//! Claude Code: a read-only parse of `<home>/.claude/.credentials.json`, the
//! file the official CLI writes, then `GET https://api.anthropic.com/api/oauth/usage`.
//! The token is read on each fetch. maestro never stores it or refreshes it.
//!
//! Known gap: on macOS, Claude Code may keep its credentials in the Keychain
//! instead of the file. v1 reads the file only. Without it, `windows` returns `None`.
//!
//! Cache: one JSON file per provider under `cache_dir`, guarded by an exclusive
//! lock file. TTL 60s. HTTP 429 pauses polling for 5 minutes. The last good
//! snapshot is returned with `stale: true`.
//!
//! Providers with no quota source (Ollama, self-hosted) return `None`. The UI
//! renders "no data" and never 0%.

mod cache;
mod credentials;
pub mod fetch;
#[cfg(test)]
mod tests;
pub mod types;

use std::fmt;
use std::path::{Path, PathBuf};

use chrono::Duration;

use crate::agent_provider::minimax::quota::{Clock, SystemClock};
use cache::CacheEntry;
use fetch::{QuotaHttp, ReqwestQuotaHttp};
use types::{ProviderWindows, TransportError};

const CLAUDE_PROVIDER_ID: &str = "claude";
const RATE_LIMIT_BACKOFF: Duration = Duration::minutes(5);

pub struct QuotaFetcher {
    home: PathBuf,
    cache_dir: PathBuf,
    http: Box<dyn QuotaHttp>,
    clock: Box<dyn Clock>,
}

impl fmt::Debug for QuotaFetcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuotaFetcher")
            .field("cache_dir", &self.cache_dir)
            .finish_non_exhaustive()
    }
}

impl QuotaFetcher {
    pub fn new(
        home: PathBuf,
        cache_dir: PathBuf,
        http: Box<dyn QuotaHttp>,
        clock: Box<dyn Clock>,
    ) -> Self {
        Self {
            home,
            cache_dir,
            http,
            clock,
        }
    }

    /// Production wiring: real HTTP client and wall clock.
    pub fn system(home: PathBuf, cache_dir: PathBuf) -> Result<Self, TransportError> {
        Ok(Self::new(
            home,
            cache_dir,
            Box::new(ReqwestQuotaHttp::new()?),
            Box::new(SystemClock),
        ))
    }

    /// Quota windows for `provider_id`. `None` means no source or no data.
    /// Never panics. Never returns 0% for a missing window.
    pub async fn windows(&self, provider_id: &str) -> Option<ProviderWindows> {
        if provider_id != CLAUDE_PROVIDER_ID {
            return None;
        }
        let now = self.clock.now();
        let cache_file = self.cache_dir.join(format!("{provider_id}.quota.json"));
        let lock_file = self.cache_dir.join(format!("{provider_id}.quota.lock"));

        // Cache is best-effort: an unusable dir degrades to uncached fetches.
        if let Err(err) = std::fs::create_dir_all(&self.cache_dir) {
            tracing::warn!(error = %err, "failed to create quota cache dir");
        }
        let _guard = match cache::try_lock(&lock_file) {
            Ok(cache::LockOutcome::Acquired(guard)) => Some(guard),
            Ok(cache::LockOutcome::Busy) => {
                // Another process is fetching. Serve what it wrote, do not poll.
                let entry = cache::load(&cache_file);
                return entry.snapshot(entry.fresh(now).is_none());
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to lock quota cache; fetching unlocked");
                None
            }
        };

        let mut entry = cache::load(&cache_file);
        if let Some(fresh) = entry.fresh(now) {
            return Some(fresh);
        }
        if entry.in_backoff(now) {
            return entry.snapshot(true);
        }
        let token = credentials::load_claude_access_token(&self.home)?;

        match self.http.get_usage(token.expose()).await {
            Ok(response) if response.status == 200 => match fetch::parse_usage(&response.body) {
                Some(windows) => {
                    entry = CacheEntry {
                        fetched_at: Some(now),
                        windows: Some(windows.clone()),
                        backoff_until: None,
                    };
                    persist(&cache_file, &entry);
                    Some(windows)
                }
                None => {
                    tracing::warn!("quota response had no parseable window");
                    entry.snapshot(true)
                }
            },
            Ok(response) if response.status == 429 => {
                entry.backoff_until = Some(now + RATE_LIMIT_BACKOFF);
                persist(&cache_file, &entry);
                entry.snapshot(true)
            }
            Ok(response) => {
                tracing::warn!(status = response.status, "quota fetch failed");
                entry.snapshot(true)
            }
            Err(err) => {
                tracing::warn!(error = %err, "quota fetch failed");
                entry.snapshot(true)
            }
        }
    }
}

fn persist(path: &Path, entry: &CacheEntry) {
    if let Err(err) = cache::store(path, entry) {
        tracing::warn!(error = %err, "failed to write quota cache");
    }
}
