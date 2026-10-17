//! Failure streaks for the usage fetch (issue #1065). A source that keeps
//! failing (401, 5xx, offline) warns once. Later failures log at debug. A
//! successful fetch ends the streak, so the next failure warns again.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Log level chosen for one failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Level {
    Warn,
    Debug,
}

/// Tracks whether a failure streak is open. Counts reports by level, so a
/// test can check the level without a tracing subscriber.
#[derive(Debug, Default)]
pub(super) struct FailureLog {
    in_streak: AtomicBool,
    warns: AtomicUsize,
    debugs: AtomicUsize,
}

impl FailureLog {
    /// The first failure of a streak is `Warn`. The rest are `Debug`.
    pub(super) fn record_failure(&self) -> Level {
        if self.in_streak.swap(true, Ordering::Relaxed) {
            self.debugs.fetch_add(1, Ordering::Relaxed);
            Level::Debug
        } else {
            self.warns.fetch_add(1, Ordering::Relaxed);
            Level::Warn
        }
    }

    /// A successful fetch ends the streak.
    pub(super) fn record_success(&self) {
        self.in_streak.store(false, Ordering::Relaxed);
    }

    /// `(warn reports, debug reports)` so far.
    pub(super) fn counts(&self) -> (usize, usize) {
        (
            self.warns.load(Ordering::Relaxed),
            self.debugs.load(Ordering::Relaxed),
        )
    }
}
