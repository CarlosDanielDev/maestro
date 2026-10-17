//! Per-provider snapshot cache with a cross-process lock file.
//!
//! Cache files hold windows and backoff times only. Never credentials.
//! On unix the cache dir is `0o700` and the cache and lock files are `0o600`.

use std::fs::{DirBuilder, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::types::ProviderWindows;

/// A snapshot younger than this is served without a fetch. It is also the
/// minimum gap between two attempts, so a failing provider is not hammered.
pub(super) const TTL: Duration = Duration::seconds(60);
/// How long a provider is left alone after HTTP 429.
pub(super) const RATE_LIMIT_BACKOFF: Duration = Duration::minutes(5);

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct CacheEntry {
    pub fetched_at: Option<DateTime<Utc>>,
    pub windows: Option<ProviderWindows>,
    pub backoff_until: Option<DateTime<Utc>>,
    pub attempted_at: Option<DateTime<Utc>>,
}

/// True when `at` is no later than `now` and less than `span` before it.
/// A timestamp in the future (clock moved backwards) is never "recent".
fn within(now: DateTime<Utc>, at: DateTime<Utc>, span: Duration) -> bool {
    let age = now - at;
    age >= Duration::zero() && age < span
}

impl CacheEntry {
    /// Windows from the last good fetch, if that fetch is within TTL.
    pub(super) fn fresh(&self, now: DateTime<Utc>) -> Option<ProviderWindows> {
        if within(now, self.fetched_at?, TTL) {
            self.snapshot(false)
        } else {
            None
        }
    }

    /// Last good windows with the given staleness flag. `None` when never fetched.
    pub(super) fn snapshot(&self, stale: bool) -> Option<ProviderWindows> {
        self.windows
            .clone()
            .map(|windows| ProviderWindows { stale, ..windows })
    }

    /// True while polling is paused by HTTP 429. The backoff is measured from
    /// the attempt that caused it. Anything further out than
    /// `attempted_at + RATE_LIMIT_BACKOFF` is corrupt, so it is clamped there.
    pub(super) fn in_backoff(&self, now: DateTime<Utc>) -> bool {
        let (Some(until), Some(attempted)) = (self.backoff_until, self.attempted_at) else {
            return false;
        };
        now < until.min(attempted + RATE_LIMIT_BACKOFF)
    }

    /// True when a fetch must not happen yet: a 429 backoff is active, or the
    /// last attempt (successful or not) was within TTL.
    pub(super) fn retry_blocked(&self, now: DateTime<Utc>) -> bool {
        self.in_backoff(now) || self.attempted_at.is_some_and(|at| within(now, at, TTL))
    }
}

/// Missing, unreadable, non-UTF8, truncated, or corrupt files read as an empty entry.
pub(super) fn load(path: &Path) -> CacheEntry {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Writes through a per-process temp file and a rename. Readers never see a
/// half-written file, and two processes never rename each other's temp file.
pub(super) fn store(path: &Path, entry: &CacheEntry) -> io::Result<()> {
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let raw = serde_json::to_string(entry).map_err(io::Error::other)?;
    let mut file = private_options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(raw.as_bytes())?;
    std::fs::rename(tmp, path)
}

/// Creates the cache dir (and parents) with `0o700` on unix.
pub(super) fn ensure_dir(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

/// Open options that create files as `0o600` on unix.
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

/// Holds the exclusive lock until dropped. The OS releases it on drop.
pub(super) struct LockGuard {
    _file: File,
}

pub(super) enum LockOutcome {
    Acquired(LockGuard),
    /// Another process holds the lock. Caller must not fetch.
    Busy,
}

pub(super) fn try_lock(path: &Path) -> io::Result<LockOutcome> {
    let file = private_options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(LockOutcome::Acquired(LockGuard { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(LockOutcome::Busy),
        Err(TryLockError::Error(err)) => Err(err),
    }
}
