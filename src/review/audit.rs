//! Audit logger for review-flow events.
//!
//! Every accept / reject / apply / bypass-toggle is funneled through this
//! trait so the activity log captures a tamper-evident trail. Required by
//! both #327 (manual accept) and #328 (bypass auto-accept).

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #327 / #328. Loggers are wired by the App
// in Phase 2; constructors and the trait are tests-only until then.
#![allow(dead_code)]

use crate::review::bypass::BypassSource;
use crate::review::types::{ConcernId, PrNumber};
use chrono::{DateTime, Utc};

/// One audit-log row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub at: DateTime<Utc>,
    pub pr_number: PrNumber,
    pub concern_id: Option<ConcernId>,
    pub event: AuditEvent,
}

/// Categorical kind of an audited event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// User (or bypass mode) accepted a concern.
    Accepted { auto: bool },
    /// User rejected a concern.
    Rejected,
    /// `ChangeApplier` successfully applied a concern's diff and committed.
    Applied { commit_sha: String },
    /// Bypass mode toggled.
    BypassToggled { active: bool, source: BypassSource },
}

/// Trait so callers can swap the production logger for `MemoryAuditLogger`
/// in tests.
pub trait AuditLogger: Send + Sync {
    fn record(&self, entry: AuditEntry);
}

/// In-memory logger used by unit tests.
#[derive(Default)]
pub struct MemoryAuditLogger {
    entries: std::sync::Mutex<Vec<AuditEntry>>,
}

impl MemoryAuditLogger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AuditLogger for MemoryAuditLogger {
    fn record(&self, entry: AuditEntry) {
        if let Ok(mut g) = self.entries.lock() {
            g.push(entry);
        }
    }
}

/// Production logger that emits via `tracing` for persistent capture.
/// Non-allocating in the hot path beyond the format args.
pub struct TracingAuditLogger;

impl AuditLogger for TracingAuditLogger {
    fn record(&self, entry: AuditEntry) {
        match &entry.event {
            AuditEvent::Accepted { auto } => tracing::info!(
                pr = %entry.pr_number,
                concern = ?entry.concern_id,
                auto,
                "review.audit.accepted"
            ),
            AuditEvent::Rejected => tracing::info!(
                pr = %entry.pr_number,
                concern = ?entry.concern_id,
                "review.audit.rejected"
            ),
            AuditEvent::Applied { commit_sha } => tracing::info!(
                pr = %entry.pr_number,
                concern = ?entry.concern_id,
                sha = %commit_sha,
                "review.audit.applied"
            ),
            AuditEvent::BypassToggled { active, source } => tracing::warn!(
                pr = %entry.pr_number,
                active,
                source = source.label(),
                "review.audit.bypass_toggled"
            ),
        }
    }
}

/// Convenience constructors.
impl AuditEntry {
    pub fn accepted(pr: PrNumber, concern: ConcernId, auto: bool) -> Self {
        Self {
            at: Utc::now(),
            pr_number: pr,
            concern_id: Some(concern),
            event: AuditEvent::Accepted { auto },
        }
    }

    pub fn rejected(pr: PrNumber, concern: ConcernId) -> Self {
        Self {
            at: Utc::now(),
            pr_number: pr,
            concern_id: Some(concern),
            event: AuditEvent::Rejected,
        }
    }

    pub fn applied(pr: PrNumber, concern: ConcernId, commit_sha: impl Into<String>) -> Self {
        Self {
            at: Utc::now(),
            pr_number: pr,
            concern_id: Some(concern),
            event: AuditEvent::Applied {
                commit_sha: commit_sha.into(),
            },
        }
    }

    pub fn bypass_toggled(pr: PrNumber, active: bool, source: BypassSource) -> Self {
        Self {
            at: Utc::now(),
            pr_number: pr,
            concern_id: None,
            event: AuditEvent::BypassToggled { active, source },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_logger_records_one_entry() {
        let logger = MemoryAuditLogger::new();
        logger.record(AuditEntry::accepted(PrNumber(1), ConcernId::new(), false));
        assert_eq!(logger.len(), 1);
    }

    #[test]
    fn memory_logger_preserves_insertion_order() {
        let logger = MemoryAuditLogger::new();
        let id1 = ConcernId::new();
        let id2 = ConcernId::new();
        logger.record(AuditEntry::accepted(PrNumber(1), id1, false));
        logger.record(AuditEntry::rejected(PrNumber(1), id2));
        let entries = logger.entries();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0].event, AuditEvent::Accepted { .. }));
        assert!(matches!(entries[1].event, AuditEvent::Rejected));
    }

    #[test]
    fn audit_entry_timestamp_is_set() {
        let entry = AuditEntry::accepted(PrNumber(1), ConcernId::new(), true);
        let now = Utc::now();
        let drift = (now - entry.at).num_seconds().abs();
        assert!(drift < 5, "timestamp should be near `now`");
    }

    #[test]
    fn auto_accepted_flag_round_trips() {
        let entry = AuditEntry::accepted(PrNumber(1), ConcernId::new(), true);
        assert!(matches!(entry.event, AuditEvent::Accepted { auto: true }));
    }

    #[test]
    fn applied_event_carries_commit_sha() {
        let entry = AuditEntry::applied(PrNumber(7), ConcernId::new(), "abc123");
        assert!(matches!(
            entry.event,
            AuditEvent::Applied { ref commit_sha } if commit_sha == "abc123"
        ));
    }

    #[test]
    fn bypass_toggled_event_records_source() {
        let entry = AuditEntry::bypass_toggled(PrNumber(1), true, BypassSource::Cli);
        assert!(matches!(
            entry.event,
            AuditEvent::BypassToggled {
                active: true,
                source: BypassSource::Cli
            }
        ));
        assert!(entry.concern_id.is_none());
    }

    #[test]
    fn tracing_logger_does_not_panic_on_any_event() {
        let logger = TracingAuditLogger;
        logger.record(AuditEntry::accepted(PrNumber(1), ConcernId::new(), false));
        logger.record(AuditEntry::rejected(PrNumber(1), ConcernId::new()));
        logger.record(AuditEntry::applied(PrNumber(1), ConcernId::new(), "x"));
        logger.record(AuditEntry::bypass_toggled(
            PrNumber(1),
            false,
            BypassSource::Tui,
        ));
    }
}
