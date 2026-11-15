//! Apply review concerns as commits (#327).
//!
//! `ChangeApplier` is a trait so the production `GitChangeApplier` (which
//! shells out to `git`) can be swapped for an in-memory fake in tests.
//! Errors are typed at the seam (RUST-GUARDRAILS §2) so callers can branch
//! on `PatchFailed` vs. `CommitFailed` vs. `NothingToApply` without parsing
//! free-form strings.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #327. The trait + InMemoryChangeApplier
// fake are exercised by tests; the real GitChangeApplier and call sites
// land in Phase 2 (security review §1 must be addressed first).
#![allow(dead_code)]

use crate::review::types::{Concern, ConcernId};

/// Outcome of applying a single concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChange {
    pub concern_id: ConcernId,
    pub commit_sha: String,
}

/// Errors raised during `apply()`. Each variant maps to a distinct
/// recovery path in the TUI.
#[derive(Debug, PartialEq, Eq)]
pub enum ChangeApplyError {
    /// Concern has no `suggested_diff` payload — nothing the applier can do.
    NothingToApply,
    /// `git apply --check` rejected the patch (conflicting hunks, file
    /// missing, etc.). Carries stderr.
    PatchFailed(String),
    /// A `git` subprocess returned non-zero. Carries the step name and
    /// stderr text.
    GitCommandFailed { step: &'static str, stderr: String },
    /// `HEAD` moved between read and apply — applier refuses to commit.
    HeadMoved { before: String, after: String },
}

impl std::fmt::Display for ChangeApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToApply => write!(f, "concern has no suggested_diff"),
            Self::PatchFailed(s) => write!(f, "git apply --check rejected the patch: {s}"),
            Self::GitCommandFailed { step, stderr } => {
                write!(f, "git {step} failed: {stderr}")
            }
            Self::HeadMoved { before, after } => write!(
                f,
                "HEAD moved during apply (before={before}, after={after}); refusing to commit"
            ),
        }
    }
}

impl std::error::Error for ChangeApplyError {}

/// Trait so callers can swap the real `git`-shelling impl for a fake.
pub trait ChangeApplier: Send + Sync {
    fn apply(&self, concern: &Concern) -> Result<AppliedChange, ChangeApplyError>;
}

/// In-memory fake used by unit tests.
#[derive(Default)]
pub struct InMemoryChangeApplier {
    applied: std::sync::Mutex<Vec<AppliedChange>>,
    fail_with: std::sync::Mutex<Option<ChangeApplyError>>,
}

impl InMemoryChangeApplier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_next(&self, err: ChangeApplyError) {
        if let Ok(mut g) = self.fail_with.lock() {
            *g = Some(err);
        }
    }

    pub fn applied(&self) -> Vec<AppliedChange> {
        self.applied.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl ChangeApplier for InMemoryChangeApplier {
    fn apply(&self, concern: &Concern) -> Result<AppliedChange, ChangeApplyError> {
        if let Ok(mut g) = self.fail_with.lock()
            && let Some(err) = g.take()
        {
            return Err(err);
        }
        if concern.suggested_diff.is_none() {
            return Err(ChangeApplyError::NothingToApply);
        }
        let change = AppliedChange {
            concern_id: concern.id,
            commit_sha: format!("fake-{}", concern.id.0.simple()),
        };
        if let Ok(mut g) = self.applied.lock() {
            g.push(change.clone());
        }
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::types::{ConcernStatus, Severity};
    use std::path::PathBuf;

    fn concern_with_diff(diff: Option<&str>) -> Concern {
        Concern {
            id: ConcernId::new(),
            severity: Severity::Warning,
            file: PathBuf::from("src/x.rs"),
            line: Some(1),
            message: "fix".into(),
            suggested_diff: diff.map(|s| s.to_string()),
            status: ConcernStatus::Pending,
        }
    }

    #[test]
    fn apply_concern_with_diff_records_change() {
        let applier = InMemoryChangeApplier::new();
        let c = concern_with_diff(Some("@@ -1 +1 @@\n-a\n+b"));
        let change = applier.apply(&c).expect("should apply");
        assert_eq!(change.concern_id, c.id);
        assert!(change.commit_sha.starts_with("fake-"));
        assert_eq!(applier.applied().len(), 1);
    }

    #[test]
    fn apply_concern_without_diff_returns_nothing_to_apply() {
        let applier = InMemoryChangeApplier::new();
        let c = concern_with_diff(None);
        assert_eq!(applier.apply(&c), Err(ChangeApplyError::NothingToApply));
    }

    #[test]
    fn apply_propagates_injected_failure() {
        let applier = InMemoryChangeApplier::new();
        applier.fail_next(ChangeApplyError::PatchFailed("hunk #2 failed".into()));
        let c = concern_with_diff(Some("x"));
        match applier.apply(&c) {
            Err(ChangeApplyError::PatchFailed(msg)) => assert!(msg.contains("hunk")),
            other => panic!("expected PatchFailed, got {other:?}"),
        }
    }

    #[test]
    fn apply_failure_does_not_record_change() {
        let applier = InMemoryChangeApplier::new();
        applier.fail_next(ChangeApplyError::GitCommandFailed {
            step: "commit",
            stderr: "boom".into(),
        });
        let c = concern_with_diff(Some("x"));
        let _ = applier.apply(&c);
        assert!(applier.applied().is_empty());
    }

    #[test]
    fn apply_multiple_concerns_records_each() {
        let applier = InMemoryChangeApplier::new();
        for _ in 0..3 {
            let c = concern_with_diff(Some("d"));
            applier.apply(&c).expect("apply");
        }
        assert_eq!(applier.applied().len(), 3);
    }

    #[test]
    fn head_moved_error_displays_both_shas() {
        let err = ChangeApplyError::HeadMoved {
            before: "abc".into(),
            after: "def".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("abc"));
        assert!(msg.contains("def"));
    }
}
