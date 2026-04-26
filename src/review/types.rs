//! Structured types for the PR review automation pipeline (#327).
//!
//! `ReviewReport` is the parsed form of a `/review` slash-command output. It
//! flows from `review::parse` → `review::audit` → `tui::screens::pr_review`
//! and back into `review::apply` when a user (or bypass mode) accepts a
//! concern.
//!
//! `#[serde(deny_unknown_fields)]` is intentional: see RUST-GUARDRAILS §6.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #327. TUI panel + session-manager wiring
// land in Phase 2; until then every public item is exercised only by tests.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Severity of a single review concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Suggestion,
    Warning,
    Critical,
}

impl Severity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Suggestion => "suggestion",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// Newtype for a review-concern identifier so we cannot mix it with PR
/// numbers or any other UUID-bearing field (Calisthenics rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConcernId(pub Uuid);

impl ConcernId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConcernId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConcernId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Newtype for a GitHub PR number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrNumber(pub u64);

impl std::fmt::Display for PrNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Disposition of a single concern as it moves through the review UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConcernStatus {
    Pending,
    Accepted,
    Rejected,
    Applied,
}

/// One entry in a `ReviewReport`.
///
/// `suggested_diff` is an optional unified-diff fragment that the user can
/// apply as a single commit when they accept the concern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Concern {
    pub id: ConcernId,
    pub severity: Severity,
    pub file: PathBuf,
    pub line: Option<u32>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_diff: Option<String>,
    #[serde(default = "default_pending")]
    pub status: ConcernStatus,
}

const fn default_pending() -> ConcernStatus {
    ConcernStatus::Pending
}

/// Top-level structured review report posted (and re-parsed) on a PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewReport {
    /// Schema version. Bumped on breaking changes to the parse contract.
    pub version: u8,
    pub pr_number: PrNumber,
    pub reviewer: String,
    pub concerns: Vec<Concern>,
}

impl ReviewReport {
    pub const SCHEMA_VERSION: u8 = 1;

    /// Construct an empty report with the current schema version pinned.
    pub fn new(pr_number: PrNumber, reviewer: impl Into<String>) -> Self {
        Self {
            version: Self::SCHEMA_VERSION,
            pr_number,
            reviewer: reviewer.into(),
            concerns: Vec::new(),
        }
    }

    /// Count concerns in each severity bucket. Returned as
    /// `(critical, warning, suggestion)`.
    pub fn severity_counts(&self) -> (usize, usize, usize) {
        let mut counts = (0usize, 0usize, 0usize);
        for c in &self.concerns {
            match c.severity {
                Severity::Critical => counts.0 += 1,
                Severity::Warning => counts.1 += 1,
                Severity::Suggestion => counts.2 += 1,
            }
        }
        counts
    }
}

/// Errors raised when attempting an illegal `ConcernStatus` transition.
#[derive(Debug, PartialEq, Eq)]
pub enum StatusTransitionError {
    Illegal {
        from: ConcernStatus,
        to: ConcernStatus,
    },
}

impl std::fmt::Display for StatusTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Illegal { from, to } => {
                write!(f, "cannot transition from {from:?} to {to:?}")
            }
        }
    }
}

impl std::error::Error for StatusTransitionError {}

impl Concern {
    /// Move the concern to a new status, enforcing the lifecycle:
    ///
    /// `Pending → {Accepted, Rejected}`
    /// `Accepted → Applied`
    /// `{Rejected, Applied}` are terminal.
    pub fn transition(&mut self, to: ConcernStatus) -> Result<(), StatusTransitionError> {
        let allowed = matches!(
            (self.status, to),
            (ConcernStatus::Pending, ConcernStatus::Accepted)
                | (ConcernStatus::Pending, ConcernStatus::Rejected)
                | (ConcernStatus::Accepted, ConcernStatus::Applied)
        );
        if !allowed {
            return Err(StatusTransitionError::Illegal {
                from: self.status,
                to,
            });
        }
        self.status = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_concern(severity: Severity) -> Concern {
        Concern {
            id: ConcernId::new(),
            severity,
            file: PathBuf::from("src/foo.rs"),
            line: Some(10),
            message: "Consider this".into(),
            suggested_diff: None,
            status: ConcernStatus::Pending,
        }
    }

    #[test]
    fn severity_round_trip_serde() {
        for sev in [Severity::Critical, Severity::Warning, Severity::Suggestion] {
            let json = serde_json::to_string(&sev).expect("serialize severity");
            let back: Severity = serde_json::from_str(&json).expect("deserialize severity");
            assert_eq!(sev, back);
        }
    }

    #[test]
    fn severity_ordering_matches_priority() {
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Suggestion);
    }

    #[test]
    fn review_report_rejects_unknown_fields() {
        let json = r#"{
          "version": 1,
          "pr_number": 42,
          "reviewer": "x",
          "concerns": [],
          "bogus": 1
        }"#;
        let result: Result<ReviewReport, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "deny_unknown_fields must reject extra keys"
        );
    }

    #[test]
    fn concern_rejects_unknown_fields() {
        let json = format!(
            r#"{{
              "id": "{}",
              "severity": "warning",
              "file": "a.rs",
              "line": 1,
              "message": "x",
              "extra": true
            }}"#,
            Uuid::new_v4()
        );
        let result: Result<Concern, _> = serde_json::from_str(&json);
        assert!(result.is_err(), "extra field on Concern must error");
    }

    #[test]
    fn review_report_round_trip_preserves_concerns() {
        let mut report = ReviewReport::new(PrNumber(7), "claude");
        report.concerns.push(make_concern(Severity::Critical));
        report.concerns.push(make_concern(Severity::Warning));
        let json = serde_json::to_string(&report).expect("serialize");
        let back: ReviewReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    #[test]
    fn severity_counts_buckets_correctly() {
        let mut report = ReviewReport::new(PrNumber(1), "x");
        report.concerns.push(make_concern(Severity::Critical));
        report.concerns.push(make_concern(Severity::Critical));
        report.concerns.push(make_concern(Severity::Warning));
        report.concerns.push(make_concern(Severity::Suggestion));
        assert_eq!(report.severity_counts(), (2, 1, 1));
    }

    #[test]
    fn transition_pending_to_accepted_then_applied_succeeds() {
        let mut c = make_concern(Severity::Warning);
        c.transition(ConcernStatus::Accepted)
            .expect("pending→accepted");
        c.transition(ConcernStatus::Applied)
            .expect("accepted→applied");
        assert_eq!(c.status, ConcernStatus::Applied);
    }

    #[test]
    fn transition_accepted_to_rejected_is_illegal() {
        let mut c = make_concern(Severity::Warning);
        c.transition(ConcernStatus::Accepted).expect("setup");
        let err = c.transition(ConcernStatus::Rejected).unwrap_err();
        assert!(matches!(err, StatusTransitionError::Illegal { .. }));
    }

    #[test]
    fn transition_applied_is_terminal() {
        let mut c = make_concern(Severity::Warning);
        c.transition(ConcernStatus::Accepted).expect("setup");
        c.transition(ConcernStatus::Applied).expect("setup");
        assert!(c.transition(ConcernStatus::Pending).is_err());
        assert!(c.transition(ConcernStatus::Accepted).is_err());
    }

    #[test]
    fn schema_version_is_pinned_in_constructor() {
        let r = ReviewReport::new(PrNumber(1), "x");
        assert_eq!(r.version, ReviewReport::SCHEMA_VERSION);
    }

    #[test]
    fn pr_number_displays_as_bare_integer() {
        assert_eq!(PrNumber(42).to_string(), "42");
    }
}
