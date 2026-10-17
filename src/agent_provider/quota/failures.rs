//! Failure reports for the quota fetch and cache (issue #1065). Each failure
//! kind has a key. The first failure of a streak under a key is `Warn`. Later
//! failures under that key are `Debug`. A success under the key ends the streak.

use std::collections::HashSet;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Log level chosen for one failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Level {
    Warn,
    Debug,
}

/// Where reports go. Production logs with tracing. Tests record them.
pub(super) trait FailureSink: Send + Sync + fmt::Debug {
    fn report(&self, level: Level, key: &'static str, what: &str);
}

#[derive(Debug, Default)]
pub(super) struct TracingSink;

impl FailureSink for TracingSink {
    fn report(&self, level: Level, _key: &'static str, what: &str) {
        match level {
            Level::Warn => tracing::warn!("{what}"),
            Level::Debug => tracing::debug!("{what}"),
        }
    }
}

/// Open streaks per key, plus the sink that receives each report.
#[derive(Debug)]
pub(super) struct FailureLog {
    open: Mutex<HashSet<&'static str>>,
    sink: Box<dyn FailureSink>,
}

impl Default for FailureLog {
    fn default() -> Self {
        Self::new(Box::new(TracingSink))
    }
}

impl FailureLog {
    pub(super) fn new(sink: Box<dyn FailureSink>) -> Self {
        Self {
            open: Mutex::new(HashSet::new()),
            sink,
        }
    }

    /// Reports one failure under `key`. The first of a streak warns.
    pub(super) fn failed(&self, key: &'static str, what: &str) {
        let opened = self.open().insert(key);
        let level = if opened { Level::Warn } else { Level::Debug };
        self.sink.report(level, key, what);
    }

    /// A success under `key` ends its streak.
    pub(super) fn succeeded(&self, key: &'static str) {
        self.open().remove(key);
    }

    fn open(&self) -> MutexGuard<'_, HashSet<&'static str>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
