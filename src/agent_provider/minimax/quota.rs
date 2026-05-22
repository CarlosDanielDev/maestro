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
const SCHEMA_VERSION: u32 = 1;

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

    /// Open or create with an injected clock + limit, for tests.
    pub fn open_with(
        path: PathBuf,
        clock: Box<dyn Clock>,
        limit: u32,
    ) -> Result<Self, MinimaxQuotaError> {
        let state = if path.exists() {
            load_state(&path)?
        } else {
            QuotaState {
                schema_version: SCHEMA_VERSION,
                requests: VecDeque::new(),
            }
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
        let now = self.clock.now();
        let cutoff = now - FIVE_HOUR_WINDOW;
        let snapshot = {
            let mut guard = self.state.lock().expect("quota mutex poisoned");
            while let Some(front) = guard.requests.front()
                && *front < cutoff
            {
                guard.requests.pop_front();
            }
            guard.requests.push_back(now);
            QuotaState {
                schema_version: guard.schema_version,
                requests: guard.requests.clone(),
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
    let state: QuotaState =
        serde_json::from_str(&buf).map_err(|source| MinimaxQuotaError::Malformed {
            path: path.to_path_buf(),
            source,
        })?;
    if state.schema_version != SCHEMA_VERSION {
        return Err(MinimaxQuotaError::UnknownSchemaVersion {
            path: path.to_path_buf(),
            found: state.schema_version,
        });
    }
    Ok(state)
}

fn save_state(path: &Path, state: &QuotaState) -> Result<(), MinimaxQuotaError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| MinimaxQuotaError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut tmp_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|source| MinimaxQuotaError::Io {
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
    let write_result = tmp_file.write_all(&json);
    let _ = tmp_file.unlock();
    write_result.map_err(|source| MinimaxQuotaError::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| MinimaxQuotaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// A clock backed by `Arc<AtomicI64>` so tests can advance time after
    /// the clock has been boxed into a `MinimaxQuota`. Construct via
    /// `FakeClock::shared(unix_secs)` which returns both an `Arc` to share
    /// with the test and a fresh `FakeClockHandle` to box into the quota.
    #[derive(Debug)]
    struct FakeClockHandle(Arc<AtomicI64>);
    impl Clock for FakeClockHandle {
        fn now(&self) -> DateTime<Utc> {
            chrono::DateTime::from_timestamp(self.0.load(Ordering::SeqCst), 0)
                .expect("valid timestamp")
        }
    }

    fn shared_clock(unix_secs: i64) -> (Arc<AtomicI64>, Box<dyn Clock>) {
        let shared = Arc::new(AtomicI64::new(unix_secs));
        let handle = FakeClockHandle(Arc::clone(&shared));
        (shared, Box::new(handle))
    }

    fn tmp_path(test_name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("minimax-quota-{test_name}.json"));
        (dir, path)
    }

    #[test]
    fn returns_ok_below_warn_threshold() {
        let (_dir, path) = tmp_path("ok");
        let (_clock, handle) = shared_clock(0);
        let quota = MinimaxQuota::open_with(path, handle, 100).unwrap();
        for _ in 0..79 {
            quota.record().unwrap();
        }
        assert!(matches!(quota.check(), QuotaStatus::Ok { .. }));
    }

    #[test]
    fn returns_warn_at_80_percent() {
        let (_dir, path) = tmp_path("warn80");
        let (_clock, handle) = shared_clock(0);
        let quota = MinimaxQuota::open_with(path, handle, 100).unwrap();
        for _ in 0..80 {
            quota.record().unwrap();
        }
        assert!(matches!(quota.check(), QuotaStatus::Warn { pct: 80 }));
    }

    #[test]
    fn returns_refused_at_95_percent() {
        let (_dir, path) = tmp_path("refuse95");
        let (_clock, handle) = shared_clock(0);
        let quota = MinimaxQuota::open_with(path, handle, 100).unwrap();
        for _ in 0..95 {
            quota.record().unwrap();
        }
        assert!(matches!(quota.check(), QuotaStatus::Refused { pct: 95 }));
    }

    #[test]
    fn evicts_samples_older_than_five_hours_on_record() {
        let (_dir, path) = tmp_path("evict");
        let (clock, handle) = shared_clock(0);
        let quota = MinimaxQuota::open_with(path, handle, 100).unwrap();

        // Fill 80 requests at t=0; window is full to 80%.
        for _ in 0..80 {
            quota.record().unwrap();
        }
        assert!(matches!(quota.check(), QuotaStatus::Warn { pct: 80 }));

        // Advance clock 6 hours; old samples now outside the 5h window.
        clock.fetch_add(6 * 3_600, Ordering::SeqCst);
        // `check()` filters by cutoff without persisting.
        assert!(matches!(quota.check(), QuotaStatus::Ok { pct: 0 }));

        // Record a new request — eviction prunes the old ones.
        quota.record().unwrap();
        assert!(matches!(quota.check(), QuotaStatus::Ok { pct: 1 }));
    }

    #[test]
    fn persistence_round_trip_via_atomic_rename() {
        let (_dir, path) = tmp_path("persist");
        {
            let (_clock, handle) = shared_clock(0);
            let quota = MinimaxQuota::open_with(path.clone(), handle, 100).unwrap();
            for _ in 0..10 {
                quota.record().unwrap();
            }
        }
        // Re-open from disk; samples must survive.
        let (_clock2, handle2) = shared_clock(0);
        let reopened = MinimaxQuota::open_with(path, handle2, 100).unwrap();
        assert_eq!(reopened.current_pct(), 10);
    }

    #[test]
    fn unknown_schema_version_returns_error() {
        let (_dir, path) = tmp_path("schema");
        std::fs::write(&path, r#"{"schema_version": 99, "requests": []}"#).unwrap();
        let (_clock, handle) = shared_clock(0);
        let err =
            MinimaxQuota::open_with(path, handle, 100).expect_err("schema_version=99 should fail");
        assert!(matches!(
            err,
            MinimaxQuotaError::UnknownSchemaVersion { found: 99, .. }
        ));
    }

    #[test]
    fn malformed_json_returns_error() {
        let (_dir, path) = tmp_path("malformed");
        std::fs::write(&path, "not json at all").unwrap();
        let (_clock, handle) = shared_clock(0);
        let err = MinimaxQuota::open_with(path, handle, 100).expect_err("malformed should fail");
        assert!(matches!(err, MinimaxQuotaError::Malformed { .. }));
    }

    #[test]
    fn missing_file_starts_with_empty_state() {
        let (_dir, path) = tmp_path("missing");
        let (_clock, handle) = shared_clock(0);
        let quota = MinimaxQuota::open_with(path, handle, 100).unwrap();
        assert!(matches!(quota.check(), QuotaStatus::Ok { pct: 0 }));
    }

    #[test]
    fn deny_unknown_fields_rejects_extra_keys() {
        let (_dir, path) = tmp_path("unknown");
        std::fs::write(
            &path,
            r#"{"schema_version": 1, "requests": [], "future_field": 42}"#,
        )
        .unwrap();
        let (_clock, handle) = shared_clock(0);
        let err =
            MinimaxQuota::open_with(path, handle, 100).expect_err("unknown field should fail");
        assert!(matches!(err, MinimaxQuotaError::Malformed { .. }));
    }
}
