//! Bypass-mode controller (#328).
//!
//! Bypass mode passes `--dangerously-skip-permissions` to the Claude
//! subprocess and auto-accepts review corrections without user
//! confirmation.
//!
//! Safety rails enforced here:
//! - There is NO `Permanent` mode variant — illegal state is unrepresentable
//!   structurally (no enum to construct, no CLI/config path that produces it).
//! - Audit log entry on every state change.
//! - `auto_disable_after_cycle()` flips back to Off after a review cycle.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #328. The controller is constructed by the
// App in Phase 2 (CLI flag → enable_session, header indicator, auto-disable
// hook); tests exercise the state machine today.
#![allow(dead_code)]

use crate::review::audit::{AuditEntry, AuditLogger};
use crate::review::types::PrNumber;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What turned bypass mode on. Used in audit-log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassSource {
    Cli,
    Tui,
    Config,
}

impl BypassSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Tui => "tui",
            Self::Config => "config",
        }
    }
}

pub struct BypassController {
    active: AtomicBool,
    audit: Arc<dyn AuditLogger>,
}

impl BypassController {
    pub fn new(audit: Arc<dyn AuditLogger>) -> Self {
        Self {
            active: AtomicBool::new(false),
            audit,
        }
    }

    /// Turn bypass mode on for the current session.
    pub fn enable_session(&self, source: BypassSource, pr: PrNumber) {
        let was_active = self.active.swap(true, Ordering::AcqRel);
        if !was_active {
            self.audit
                .record(AuditEntry::bypass_toggled(pr, true, source));
        }
    }

    /// Turn bypass mode off. Idempotent.
    pub fn disable(&self, pr: PrNumber, source: BypassSource) {
        let was_active = self.active.swap(false, Ordering::AcqRel);
        if was_active {
            self.audit
                .record(AuditEntry::bypass_toggled(pr, false, source));
        }
    }

    /// Disable when the review cycle finishes — same as `disable` but
    /// always sourced as `tui` (the cycle-end signal originates from app
    /// state).
    pub fn auto_disable_after_cycle(&self, pr: PrNumber) {
        self.disable(pr, BypassSource::Tui);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Translate the bypass state into the Claude CLI permission-mode
    /// string. Used by the session pool to construct subprocess args.
    pub fn permission_mode(&self) -> &'static str {
        if self.is_active() {
            "bypassPermissions"
        } else {
            "default"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::audit::{AuditEvent, MemoryAuditLogger};

    fn ctl() -> (BypassController, Arc<MemoryAuditLogger>) {
        let logger = Arc::new(MemoryAuditLogger::new());
        let ctl = BypassController::new(logger.clone());
        (ctl, logger)
    }

    #[test]
    fn bypass_initially_inactive() {
        let (b, _) = ctl();
        assert!(!b.is_active());
        assert_eq!(b.permission_mode(), "default");
    }

    #[test]
    fn enable_session_makes_active_and_audits() {
        let (b, log) = ctl();
        b.enable_session(BypassSource::Cli, PrNumber(1));
        assert!(b.is_active());
        assert_eq!(b.permission_mode(), "bypassPermissions");
        let entries = log.entries();
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].event,
            AuditEvent::BypassToggled { active: true, .. }
        ));
    }

    #[test]
    fn disable_makes_inactive_and_audits() {
        let (b, log) = ctl();
        b.enable_session(BypassSource::Tui, PrNumber(1));
        b.disable(PrNumber(1), BypassSource::Tui);
        assert!(!b.is_active());
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn auto_disable_after_cycle_deactivates() {
        let (b, _) = ctl();
        b.enable_session(BypassSource::Tui, PrNumber(7));
        b.auto_disable_after_cycle(PrNumber(7));
        assert!(!b.is_active());
    }

    #[test]
    fn double_enable_is_idempotent_and_audits_once() {
        let (b, log) = ctl();
        b.enable_session(BypassSource::Cli, PrNumber(1));
        b.enable_session(BypassSource::Cli, PrNumber(1));
        assert!(b.is_active());
        assert_eq!(
            log.len(),
            1,
            "second enable should not produce another audit entry"
        );
    }

    #[test]
    fn double_disable_is_idempotent_and_audits_once() {
        let (b, log) = ctl();
        b.enable_session(BypassSource::Cli, PrNumber(1));
        b.disable(PrNumber(1), BypassSource::Cli);
        b.disable(PrNumber(1), BypassSource::Cli);
        assert_eq!(log.len(), 2, "second disable should not log again");
    }

    #[test]
    fn permission_mode_strings_are_stable() {
        let (b, _) = ctl();
        assert_eq!(b.permission_mode(), "default");
        b.enable_session(BypassSource::Tui, PrNumber(1));
        assert_eq!(b.permission_mode(), "bypassPermissions");
    }

    #[test]
    fn bypass_source_label_round_trip() {
        assert_eq!(BypassSource::Cli.label(), "cli");
        assert_eq!(BypassSource::Tui.label(), "tui");
        assert_eq!(BypassSource::Config.label(), "config");
    }
}
