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
        chrono::DateTime::from_timestamp(self.0.load(Ordering::SeqCst), 0).expect("valid timestamp")
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
    let err = MinimaxQuota::open_with(path, handle, 100).expect_err("unknown field should fail");
    assert!(matches!(err, MinimaxQuotaError::Malformed { .. }));
}
