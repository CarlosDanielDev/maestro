//! 5-hour sliding-window request quota for MiniMax.
//!
//! MiniMax's free tier exposes a 5-hour rolling 4,500-request window. This
//! module tracks recent request timestamps in memory, persists them to
//! `~/.maestro/minimax-quota.json`, and serializes cross-process access via
//! a `fs2` advisory file lock so two parallel `maestro` invocations don't
//! double-spend the same window.
//!
//! The `Clock` trait is injected so tests can drive the window deterministic
//! ally. Production uses `SystemClock` (chrono's `Utc::now`).

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

/// Default 5-hour-window request cap for the free MiniMax tier.
pub const DEFAULT_FIVE_HOUR_REQUEST_LIMIT: u32 = 4_500;
const FIVE_HOUR_WINDOW: Duration = Duration::hours(5);
const WARN_PCT: u8 = 80;
const REFUSE_PCT: u8 = 95;
const SCHEMA_VERSION: u32 = 2;

/// Injectable clock so tests can advance time without sleeping.
pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Result of a pre-spawn quota check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaStatus {
    Ok { pct: u8 },
    Warn { pct: u8 },
    Refused { pct: u8 },
}

#[derive(Debug, thiserror::Error)]
pub enum MinimaxQuotaError {
    #[error("MiniMax quota file at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("MiniMax quota file at {path} has unsupported schema_version {found}")]
    UnknownSchemaVersion { path: PathBuf, found: u32 },
    #[error("MiniMax quota file at {path} is malformed: {source}")]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct QuotaState {
    schema_version: u32,
    requests: VecDeque<DateTime<Utc>>,
    /// Count of `--force-quota` bypasses recorded in the current 5h window.
    /// Resets to 0 when window pruning leaves `requests` empty (#845).
    #[serde(default)]
    forced_count: u32,
}

/// Strict v1 shape used only by the migration path in [`load_state`].
/// v1 files have no `forced_count` field; the read shim promotes them
/// in-place to v2 with `forced_count = 0` (#845).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuotaStateV1 {
    #[allow(dead_code)]
    schema_version: u32,
    requests: VecDeque<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct MinimaxQuota {
    path: PathBuf,
    clock: Box<dyn Clock>,
    limit: u32,
    state: Mutex<QuotaState>,
}

impl MinimaxQuota {
    /// Open or create the quota file at `path`. Uses `SystemClock` and the
    /// default 4,500-request cap.
    pub fn open(path: PathBuf) -> Result<Self, MinimaxQuotaError> {
        Self::open_with(path, Box::new(SystemClock), DEFAULT_FIVE_HOUR_REQUEST_LIMIT)
    }

    /// Best-effort open at the canonical path `$HOME/.maestro/minimax-quota.json`.
    /// Returns `None` when `$HOME` is unset or the file fails to open / parse —
    /// callers fall back to a no-quota render path. Shared by the `cmd_run`
    /// spawn-gate wiring and the `cmd_dashboard` TUI-render wiring (#769).
    pub fn open_default() -> Option<Self> {
        let path = std::env::var_os("HOME").map(|home| {
            std::path::PathBuf::from(home)
                .join(".maestro")
                .join("minimax-quota.json")
        })?;
        Self::open(path).ok()
    }

    /// Open or create with an injected clock + limit, for tests.
    pub fn open_with(
        path: PathBuf,
        clock: Box<dyn Clock>,
        limit: u32,
    ) -> Result<Self, MinimaxQuotaError> {
        // Open-and-branch instead of `path.exists()` first to avoid a
        // TOCTOU window between the check and the open.
        let state = match load_state(&path) {
            Ok(state) => state,
            Err(MinimaxQuotaError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                QuotaState {
                    schema_version: SCHEMA_VERSION,
                    requests: VecDeque::new(),
                    forced_count: 0,
                }
            }
            Err(err) => return Err(err),
        };
        Ok(Self {
            path,
            clock,
            limit,
            state: Mutex::new(state),
        })
    }

    /// Check if a spawn is allowed without recording it. Returns the bucket
    /// the current window falls in: `Ok`, `Warn` (≥ 80%), or `Refused`
    /// (≥ 95%). Caller decides whether to honor a force flag.
    pub fn check(&self) -> QuotaStatus {
        let pct = self.current_pct();
        bucket(pct)
    }

    /// Record one request against the window and persist to disk. The
    /// in-memory mutex is dropped before file I/O so concurrent reads
    /// aren't blocked by a slow disk.
    pub fn record(&self) -> Result<(), MinimaxQuotaError> {
        self.record_internal(false)
    }

    /// Record one request that bypassed the refusal gate via `--force-quota`
    /// (#845). Increments `forced_count` in addition to the normal record
    /// behavior so the TUI footer can surface how many forced spawns happened
    /// in the current 5h window.
    pub fn record_forced(&self) -> Result<(), MinimaxQuotaError> {
        self.record_internal(true)
    }

    /// Current `forced_count` for the active 5h window (#845).
    pub fn forced_count(&self) -> u32 {
        self.state
            .lock()
            .expect("quota mutex poisoned")
            .forced_count
    }

    /// Configured request limit for the 5h window. Stable; exposed for the
    /// `ProviderQuotaSnapshots` adapter that surfaces quota state in the TUI
    /// (#848).
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Live request count within the active 5h window. Locks the internal
    /// mutex briefly to count entries newer than `now - 5h`. Returns 0 on
    /// poisoning (caller — typically the TUI rollup — treats absence as
    /// "no quota data" rather than crashing — #848 hard rule §2).
    pub fn used_in_window(&self) -> u32 {
        let Ok(guard) = self.state.lock() else {
            return 0;
        };
        let now = self.clock.now();
        let cutoff = now - FIVE_HOUR_WINDOW;
        let count = guard.requests.iter().filter(|ts| **ts >= cutoff).count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn record_internal(&self, forced: bool) -> Result<(), MinimaxQuotaError> {
        let now = self.clock.now();
        let cutoff = now - FIVE_HOUR_WINDOW;
        let snapshot = {
            let mut guard = self.state.lock().expect("quota mutex poisoned");
            while let Some(front) = guard.requests.front()
                && *front < cutoff
            {
                guard.requests.pop_front();
            }
            // Whole window aged out — reset the forced-spawn counter so it
            // stays scoped to the active window per #845.
            if guard.requests.is_empty() {
                guard.forced_count = 0;
            }
            guard.requests.push_back(now);
            if forced {
                guard.forced_count = guard.forced_count.saturating_add(1);
            }
            QuotaState {
                schema_version: guard.schema_version,
                requests: guard.requests.clone(),
                forced_count: guard.forced_count,
            }
        };
        save_state(&self.path, &snapshot)?;
        Ok(())
    }

    fn current_pct(&self) -> u8 {
        if self.limit == 0 {
            return 0;
        }
        let now = self.clock.now();
        let cutoff = now - FIVE_HOUR_WINDOW;
        let count = {
            let guard = self.state.lock().expect("quota mutex poisoned");
            guard.requests.iter().filter(|ts| **ts >= cutoff).count()
        };
        let pct_f = (count as f64) * 100.0 / f64::from(self.limit);
        pct_f.min(100.0).round() as u8
    }
}

fn bucket(pct: u8) -> QuotaStatus {
    if pct >= REFUSE_PCT {
        QuotaStatus::Refused { pct }
    } else if pct >= WARN_PCT {
        QuotaStatus::Warn { pct }
    } else {
        QuotaStatus::Ok { pct }
    }
}

fn load_state(path: &Path) -> Result<QuotaState, MinimaxQuotaError> {
    let mut file = File::open(path).map_err(|source| MinimaxQuotaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.lock_shared().map_err(|source| MinimaxQuotaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut buf = String::new();
    let read_result = file.read_to_string(&mut buf);
    let _ = file.unlock();
    read_result.map_err(|source| MinimaxQuotaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    // Two-stage deserialize to support v1 → v2 in-place migration (#845).
    // Parse into a typed envelope just for the version sentinel, then
    // dispatch to the version-specific shape (each with deny_unknown_fields).
    let envelope: serde_json::Value =
        serde_json::from_str(&buf).map_err(|source| MinimaxQuotaError::Malformed {
            path: path.to_path_buf(),
            source,
        })?;
    let version = envelope
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    match version {
        1 => {
            let v1: QuotaStateV1 = serde_json::from_value(envelope).map_err(|source| {
                MinimaxQuotaError::Malformed {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
            Ok(QuotaState {
                schema_version: SCHEMA_VERSION,
                requests: v1.requests,
                forced_count: 0,
            })
        }
        2 => {
            let state: QuotaState = serde_json::from_value(envelope).map_err(|source| {
                MinimaxQuotaError::Malformed {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
            Ok(state)
        }
        other => Err(MinimaxQuotaError::UnknownSchemaVersion {
            path: path.to_path_buf(),
            found: other,
        }),
    }
}

fn save_state(path: &Path, state: &QuotaState) -> Result<(), MinimaxQuotaError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| MinimaxQuotaError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    // Clean up any stale tmp file from a prior crashed write. We do NOT
    // follow symlinks here — std::fs::remove_file unlinks the symlink
    // itself rather than the target.
    let _ = std::fs::remove_file(&tmp);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut tmp_file = opts.open(&tmp).map_err(|source| MinimaxQuotaError::Io {
        path: tmp.clone(),
        source,
    })?;
    tmp_file
        .lock_exclusive()
        .map_err(|source| MinimaxQuotaError::Io {
            path: tmp.clone(),
            source,
        })?;
    let json = serde_json::to_vec_pretty(state).map_err(|source| MinimaxQuotaError::Malformed {
        path: path.to_path_buf(),
        source,
    })?;
    let write_result = tmp_file.write_all(&json).and_then(|_| tmp_file.sync_all());
    let _ = tmp_file.unlock();
    write_result.map_err(|source| MinimaxQuotaError::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| MinimaxQuotaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    // Best-effort fsync on the parent directory so the rename is durable
    // across power loss. Ignored on platforms (Windows) where the dir
    // open is unsupported.
    if let Some(parent) = path.parent()
        && let Ok(dir) = File::open(parent)
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
#[path = "quota_tests.rs"]
mod tests;
