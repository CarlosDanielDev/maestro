//! Lifecycle events emitted as an interactive session winds down (#739).
//!
//! `PrLinkedToIssue` is raised by the `/pushup` marker consumer
//! (`src/tui/app/pushup_marker.rs`) when a PR marker carries an
//! `issue_number` that matches an active interaction session. It drives
//! [`super::interaction::InteractionSession::signal_terminator`].
//!
//! `WorktreeWiped` and `SessionClosed` are scaffold for #740 (worktree
//! teardown) and #741 (terminator UI flow); they have no consumer yet, hence
//! the module-level `dead_code` allow.
#![allow(dead_code)] // Reason: WorktreeWiped/SessionClosed are scaffold for #740/#741

use super::interaction::CloseReason;
use std::path::PathBuf;

/// An event in the shutdown lifecycle of an interactive session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionLifecycleEvent {
    /// A `/pushup` PR was linked to this session's issue. Terminates the
    /// session with `CloseReason::PrCreated`.
    PrLinkedToIssue {
        pr_number: u64,
        issue_number: u64,
        owner: String,
        repo: String,
    },
    /// The session's worktree was removed (#740).
    WorktreeWiped { issue_number: u64, path: PathBuf },
    /// The session was closed for `reason` (#741).
    SessionClosed {
        issue_number: u64,
        reason: CloseReason,
    },
}
