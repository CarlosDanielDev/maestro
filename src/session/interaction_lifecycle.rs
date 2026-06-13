//! Lifecycle events emitted as an interactive session winds down (#739).
//!
//! `PrLinkedToIssue` is raised by the `/pushup` marker consumer
//! (`src/tui/app/pushup_marker.rs`) when a PR marker carries an
//! `issue_number` that matches a live interactive session. It drives
//! [`crate::session::manager::ManagedSession::signal_terminator`] (#948)
//! and the Interaction screen's terminator UI flow (#741).
//!
//! The `WorktreeWiped`/`SessionClosed` scaffold variants (#740/#741) were
//! removed in #948 — they never gained a consumer.

/// An event in the shutdown lifecycle of an interactive session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionLifecycleEvent {
    /// A `/pushup` PR was linked to this session's issue. Terminates the
    /// session (Phase 4 / #949 makes this non-terminal).
    PrLinkedToIssue {
        pr_number: u64,
        issue_number: u64,
        owner: String,
        repo: String,
    },
}
