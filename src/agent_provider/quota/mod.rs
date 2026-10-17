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
//! Cache: one JSON file per provider under `cache_dir` (`0o700` on unix), guarded
//! by an exclusive lock file (`0o600`). TTL 60s, which is also the minimum gap
//! between attempts, so a failing provider is not retried every call. HTTP 429
//! pauses polling for 5 minutes. A failed attempt serves the last good snapshot
//! with `stale: true`. A fetch that runs without the lock does not write the cache.
//!
//! Providers with no quota source (Ollama, self-hosted) return `None`. The UI
//! renders "no data" and never 0%.
//!
//! Known coupling: `Clock` and `SystemClock` are imported from
//! `agent_provider::minimax::quota`. The Claude quota code depends on the
//! MiniMax module. Moving the clock trait into a shared module is a follow-up.
//! It is not done in this slice.
//!
//! Test fixture note: `tests/fixtures/claude_oauth_usage.json` is SYNTHESIZED from
//! the shape described in issue #1064, not a live capture. The endpoint shape,
//! the percent scale, and the required headers are unverified against the live API.

pub mod alert;
pub mod badge;
mod cache;
mod credentials;
mod failures;
pub mod fetch;
pub mod pacing;
pub mod routing;
#[cfg(test)]
mod tests;
pub mod types;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::agent_provider::minimax::quota::{Clock, SystemClock};
use cache::{CacheEntry, RATE_LIMIT_BACKOFF};
use failures::{FailureLog, Level};
use fetch::{QuotaHttp, ReqwestQuotaHttp};
use types::{ProviderWindows, TransportError};

const CLAUDE_PROVIDER_ID: &str = "claude";

/// Extra gap on top of the cache TTL. A tick that lands inside the TTL window
/// is skipped, so the refresh loop must wait longer than the TTL.
const REFRESH_SLACK: std::time::Duration = std::time::Duration::from_secs(5);

/// How often a live refresh should poll. Derived from the cache TTL, so the
/// two cannot drift apart.
pub fn refresh_interval() -> std::time::Duration {
    cache::TTL.to_std().unwrap_or_default() + REFRESH_SLACK
}

pub struct QuotaFetcher {
    home: PathBuf,
    cache_dir: PathBuf,
    http: Box<dyn QuotaHttp>,
    clock: Box<dyn Clock>,
    failures: FailureLog,
}

/// Outcome of one HTTP attempt, reduced to what the cache needs.
enum Fetched {
    Windows(ProviderWindows),
    RateLimited,
    Failed,
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
            failures: FailureLog::default(),
        }
    }

    /// `(warn reports, debug reports)` for failed fetches so far.
    pub(super) fn failure_counts(&self) -> (usize, usize) {
        self.failures.counts()
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
        if let Err(err) = cache::ensure_dir(&self.cache_dir) {
            tracing::warn!(error = %err, "failed to create quota cache dir");
        }
        let guard = match cache::try_lock(&lock_file) {
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
        let locked = guard.is_some();

        let mut entry = cache::load(&cache_file);
        if let Some(fresh) = entry.fresh(now) {
            return Some(fresh);
        }
        if entry.retry_blocked(now) {
            return entry.snapshot(true);
        }
        let token = credentials::load_claude_access_token(&self.home)?;

        // Any attempt that is not a 429 must not inherit an old backoff. A fetch
        // only runs when no valid backoff blocked it, so the old value is expired or corrupt.
        entry.attempted_at = Some(now);
        entry.backoff_until = None;
        let result = match self.request(token.expose()).await {
            Fetched::Windows(windows) => {
                entry = CacheEntry {
                    fetched_at: Some(now),
                    windows: Some(windows.clone()),
                    backoff_until: None,
                    attempted_at: Some(now),
                };
                Some(windows)
            }
            Fetched::RateLimited => {
                entry.backoff_until = Some(now + RATE_LIMIT_BACKOFF);
                entry.snapshot(true)
            }
            Fetched::Failed => entry.snapshot(true),
        };
        // Only the lock holder writes. An unlocked fetch must not race the writer.
        if locked {
            persist(&cache_file, &entry);
        }
        result
    }

    async fn request(&self, token: &str) -> Fetched {
        match self.http.get_usage(token).await {
            Ok(response) if response.status == 200 => match fetch::parse_usage(&response.body) {
                Some(windows) => {
                    self.failures.record_success();
                    Fetched::Windows(windows)
                }
                None => {
                    self.report_failure("no parseable window");
                    Fetched::Failed
                }
            },
            Ok(response) if response.status == 429 => Fetched::RateLimited,
            Ok(response) => {
                self.report_failure(&format!("status {}", response.status));
                Fetched::Failed
            }
            Err(err) => {
                self.report_failure(&err.to_string());
                Fetched::Failed
            }
        }
    }
}

impl QuotaFetcher {
    /// Warn once per failure streak. Later failures log at debug.
    fn report_failure(&self, detail: &str) {
        match self.failures.record_failure() {
            Level::Warn => tracing::warn!(%detail, "quota fetch failed"),
            Level::Debug => tracing::debug!(%detail, "quota fetch still failing"),
        }
    }
}

fn persist(path: &Path, entry: &CacheEntry) {
    if let Err(err) = cache::store(path, entry) {
        tracing::warn!(error = %err, "failed to write quota cache");
    }
}
