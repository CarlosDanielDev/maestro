//! Per-provider snapshot cache with a cross-process lock file.
//!
//! Cache files hold windows and backoff times only. Never credentials.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::types::ProviderWindows;

/// A snapshot younger than this is served without a fetch.
pub(super) const TTL: Duration = Duration::seconds(60);

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct CacheEntry {
    pub fetched_at: Option<DateTime<Utc>>,
    pub windows: Option<ProviderWindows>,
    pub backoff_until: Option<DateTime<Utc>>,
}

impl CacheEntry {
    /// Windows from the last good fetch, if that fetch is within TTL.
    pub(super) fn fresh(&self, now: DateTime<Utc>) -> Option<ProviderWindows> {
        if now - self.fetched_at? < TTL {
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

    pub(super) fn in_backoff(&self, now: DateTime<Utc>) -> bool {
        self.backoff_until.is_some_and(|until| now < until)
    }
}

/// Missing, unreadable, or corrupt files read as an empty entry.
pub(super) fn load(path: &Path) -> CacheEntry {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Writes through a temp file and a rename, so readers never see a half-written file.
pub(super) fn store(path: &Path, entry: &CacheEntry) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let raw = serde_json::to_string(entry).map_err(io::Error::other)?;
    std::fs::write(&tmp, raw)?;
    std::fs::rename(tmp, path)
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
    let file = OpenOptions::new()
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
