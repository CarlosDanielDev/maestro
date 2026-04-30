use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::SessionStatus;

/// Reason for a state transition, providing audit context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionReason {
    Promoted,
    Spawned,
    StreamCompleted,
    StreamError,
    UserKill,
    UserPause,
    UserResume,
    HealthStall,
    RetryTriggered,
    GatesStarted,
    GatesFailed,
    CiFixStarted,
    ConflictPolicy,
    ContextOverflow,
    PrNeeded,
    ConflictFixStarted,
}

/// Record of a single state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTransition {
    pub from: SessionStatus,
    pub to: SessionStatus,
    pub reason: TransitionReason,
    pub timestamp: DateTime<Utc>,
}

/// Error returned when an illegal transition is attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: SessionStatus,
    pub to: SessionStatus,
}

impl std::fmt::Display for IllegalTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "illegal transition {} -> {}",
            self.from.label(),
            self.to.label()
        )
    }
}

impl std::error::Error for IllegalTransition {}

/// Observer notified of session state transitions.
#[allow(dead_code)] // Reason: transition hooks — to be wired into session lifecycle
pub trait TransitionObserver: Send {
    fn on_transition(&mut self, session_id: uuid::Uuid, transition: &SessionTransition);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{Session, SessionStatus};

    fn make_session() -> Session {
        Session::new(
            "test prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        )
    }

    // --- TransitionReason ---

    #[test]
    fn transition_reason_variants_are_distinct() {
        assert_ne!(TransitionReason::Promoted, TransitionReason::Spawned);
        assert_ne!(
            TransitionReason::StreamCompleted,
            TransitionReason::StreamError
        );
        assert_ne!(TransitionReason::UserKill, TransitionReason::UserPause);
        assert_ne!(
            TransitionReason::GatesStarted,
            TransitionReason::GatesFailed
        );
        assert_ne!(
            TransitionReason::ConflictPolicy,
            TransitionReason::ContextOverflow
        );
        assert_ne!(
            TransitionReason::PrNeeded,
            TransitionReason::ConflictFixStarted
        );
    }

    #[test]
    fn session_transition_stores_all_fields() {
        let now = Utc::now();
        let t = SessionTransition {
            from: SessionStatus::Running,
            to: SessionStatus::Completed,
            reason: TransitionReason::StreamCompleted,
            timestamp: now,
        };
        assert_eq!(t.from, SessionStatus::Running);
        assert_eq!(t.to, SessionStatus::Completed);
        assert_eq!(t.reason, TransitionReason::StreamCompleted);
    }

    #[test]
    fn illegal_transition_display_is_human_readable() {
        let e = IllegalTransition {
            from: SessionStatus::Completed,
            to: SessionStatus::Running,
        };
        let msg = e.to_string();
        assert!(!msg.is_empty());
        let lower = msg.to_lowercase();
        assert!(lower.contains("completed"));
        assert!(lower.contains("running"));
    }

    // --- valid_transitions() ---

    fn assert_transitions(status: SessionStatus, expected: &[SessionStatus]) {
        let got = status.valid_transitions();
        let mut got_sorted: Vec<SessionStatus> = got.to_vec();
        got_sorted.sort_by_key(|s| *s as u8);
        let mut exp_sorted: Vec<SessionStatus> = expected.to_vec();
        exp_sorted.sort_by_key(|s| *s as u8);
        assert_eq!(
            got_sorted, exp_sorted,
            "valid_transitions for {:?} mismatch.\n  got: {:?}\n  exp: {:?}",
            status, got_sorted, exp_sorted
        );
    }

    #[test]
    fn valid_transitions_queued() {
        assert_transitions(
            SessionStatus::Queued,
            &[
                SessionStatus::Spawning,
                SessionStatus::Killed,
                SessionStatus::CiFix,
                SessionStatus::ConflictFix,
            ],
        );
    }

    #[test]
    fn valid_transitions_spawning() {
        assert_transitions(
            SessionStatus::Spawning,
            &[
                SessionStatus::Running,
                SessionStatus::Errored,
                SessionStatus::Killed,
            ],
        );
    }

    #[test]
    fn valid_transitions_running() {
        assert_transitions(
            SessionStatus::Running,
            &[
                SessionStatus::Completed,
                SessionStatus::Errored,
                SessionStatus::Paused,
                SessionStatus::Stalled,
                SessionStatus::Killed,
                SessionStatus::GatesRunning,
                SessionStatus::NeedsPr,
                SessionStatus::CiFix,
                SessionStatus::ConflictFix,
            ],
        );
    }

    #[test]
    fn valid_transitions_paused() {
        assert_transitions(
            SessionStatus::Paused,
            &[SessionStatus::Running, SessionStatus::Killed],
        );
    }

    #[test]
    fn valid_transitions_stalled() {
        assert_transitions(
            SessionStatus::Stalled,
            &[
                SessionStatus::Retrying,
                SessionStatus::Killed,
                SessionStatus::Errored,
            ],
        );
    }

    #[test]
    fn valid_transitions_completed_is_empty() {
        assert!(SessionStatus::Completed.valid_transitions().is_empty());
    }

    #[test]
    fn valid_transitions_gates_running() {
        assert_transitions(
            SessionStatus::GatesRunning,
            &[
                SessionStatus::NeedsReview,
                SessionStatus::Completed,
                SessionStatus::Errored,
            ],
        );
    }

    #[test]
    fn valid_transitions_needs_review_is_empty() {
        assert!(SessionStatus::NeedsReview.valid_transitions().is_empty());
    }

    #[test]
    fn valid_transitions_errored() {
        assert_transitions(SessionStatus::Errored, &[SessionStatus::Retrying]);
    }

    #[test]
    fn valid_transitions_retrying() {
        assert_transitions(
            SessionStatus::Retrying,
            &[
                SessionStatus::Spawning,
                SessionStatus::Errored,
                SessionStatus::Killed,
            ],
        );
    }

    #[test]
    fn valid_transitions_ci_fix() {
        assert_transitions(
            SessionStatus::CiFix,
            &[
                SessionStatus::Spawning,
                SessionStatus::Errored,
                SessionStatus::Killed,
            ],
        );
    }

    #[test]
    fn valid_transitions_needs_pr() {
        assert_transitions(
            SessionStatus::NeedsPr,
            &[SessionStatus::Completed, SessionStatus::Errored],
        );
    }

    #[test]
    fn valid_transitions_conflict_fix() {
        assert_transitions(
            SessionStatus::ConflictFix,
            &[
                SessionStatus::Spawning,
                SessionStatus::Errored,
                SessionStatus::Killed,
            ],
        );
    }

    #[test]
    fn valid_transitions_killed_is_empty() {
        assert!(SessionStatus::Killed.valid_transitions().is_empty());
    }

    // --- can_transition_to() ---

    #[test]
    fn can_transition_to_returns_true_for_valid_target() {
        assert!(SessionStatus::Running.can_transition_to(SessionStatus::Completed));
    }

    #[test]
    fn can_transition_to_returns_false_for_invalid_target() {
        assert!(!SessionStatus::Queued.can_transition_to(SessionStatus::Completed));
    }

    #[test]
    fn can_transition_to_returns_false_from_terminal_state() {
        assert!(!SessionStatus::Completed.can_transition_to(SessionStatus::Running));
    }

    #[test]
    fn can_transition_to_all_running_targets_accepted() {
        let valid = [
            SessionStatus::Completed,
            SessionStatus::Errored,
            SessionStatus::Paused,
            SessionStatus::Stalled,
            SessionStatus::Killed,
            SessionStatus::GatesRunning,
            SessionStatus::NeedsPr,
            SessionStatus::CiFix,
            SessionStatus::ConflictFix,
        ];
        for target in valid {
            assert!(
                SessionStatus::Running.can_transition_to(target),
                "Running -> {:?} must be valid",
                target
            );
        }
    }

    #[test]
    fn can_transition_to_non_targets_from_running_rejected() {
        let invalid = [
            SessionStatus::Queued,
            SessionStatus::Spawning,
            SessionStatus::Retrying,
            SessionStatus::NeedsReview,
        ];
        for target in invalid {
            assert!(
                !SessionStatus::Running.can_transition_to(target),
                "Running -> {:?} must be invalid",
                target
            );
        }
    }

    // --- transition_to() on Session ---

    #[test]
    fn transition_to_valid_succeeds_and_updates_status() {
        let mut s = make_session();
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Spawning);
    }

    #[test]
    fn transition_to_invalid_returns_illegal_transition_error() {
        let mut s = make_session();
        let result = s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.from, SessionStatus::Queued);
        assert_eq!(err.to, SessionStatus::Completed);
    }

    #[test]
    fn transition_to_records_history_entry() {
        let mut s = make_session();
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        assert_eq!(s.transition_history.len(), 1);
        let entry = &s.transition_history[0];
        assert_eq!(entry.from, SessionStatus::Queued);
        assert_eq!(entry.to, SessionStatus::Spawning);
        assert_eq!(entry.reason, TransitionReason::Promoted);
    }

    #[test]
    fn transition_to_accumulates_history() {
        let mut s = make_session();
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        assert_eq!(s.transition_history.len(), 3);
        assert_eq!(s.transition_history[0].from, SessionStatus::Queued);
        assert_eq!(s.transition_history[1].from, SessionStatus::Spawning);
        assert_eq!(s.transition_history[2].from, SessionStatus::Running);
    }

    #[test]
    fn transition_to_failed_does_not_mutate_status() {
        let mut s = make_session();
        s.status = SessionStatus::Running;
        let _ = s.transition_to(SessionStatus::Queued, TransitionReason::Promoted);
        assert_eq!(s.status, SessionStatus::Running);
    }

    #[test]
    fn transition_to_failed_does_not_add_history_entry() {
        let mut s = make_session();
        s.status = SessionStatus::Running;
        let _ = s.transition_to(SessionStatus::Queued, TransitionReason::Promoted);
        assert!(s.transition_history.is_empty());
    }

    #[test]
    fn transition_to_terminal_state_succeeds() {
        let mut s = make_session();
        s.status = SessionStatus::Running;
        s.transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Killed);
        assert_eq!(s.transition_history.len(), 1);
    }

    #[test]
    fn transition_to_from_terminal_state_returns_error() {
        let mut s = make_session();
        s.status = SessionStatus::Killed;
        let result = s.transition_to(SessionStatus::Retrying, TransitionReason::RetryTriggered);
        assert!(result.is_err());
    }

    #[test]
    fn transition_to_errored_self_is_invalid() {
        let mut s = make_session();
        s.status = SessionStatus::Errored;
        let result = s.transition_to(SessionStatus::Errored, TransitionReason::StreamError);
        assert!(result.is_err());
    }

    #[test]
    fn transition_to_history_timestamps_are_monotonic() {
        let mut s = make_session();
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        let h = &s.transition_history;
        assert!(h[0].timestamp <= h[1].timestamp);
        assert!(h[1].timestamp <= h[2].timestamp);
    }

    // --- is_terminal() alignment ---

    #[test]
    fn completed_is_terminal() {
        assert!(SessionStatus::Completed.is_terminal());
    }

    #[test]
    fn needs_review_is_terminal() {
        assert!(SessionStatus::NeedsReview.is_terminal());
    }

    #[test]
    fn killed_is_terminal() {
        assert!(SessionStatus::Killed.is_terminal());
    }

    #[test]
    fn errored_is_not_terminal() {
        // Errored -> Retrying is valid, so Errored is no longer terminal
        assert!(!SessionStatus::Errored.is_terminal());
    }

    #[test]
    fn non_terminal_states_are_not_terminal() {
        let non_terminals = [
            SessionStatus::Queued,
            SessionStatus::Spawning,
            SessionStatus::Running,
            SessionStatus::Paused,
            SessionStatus::Stalled,
            SessionStatus::GatesRunning,
            SessionStatus::Errored,
            SessionStatus::Retrying,
            SessionStatus::CiFix,
            SessionStatus::NeedsPr,
            SessionStatus::ConflictFix,
        ];
        for s in non_terminals {
            assert!(!s.is_terminal(), "{:?} must not be terminal", s);
        }
    }

    #[test]
    fn transition_to_sets_finished_at_on_terminal() {
        let mut s = make_session();
        s.status = SessionStatus::Running;
        assert!(s.finished_at.is_none());
        s.transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();
        assert!(s.finished_at.is_some());
    }

    #[test]
    fn transition_reason_round_trips_via_serde() {
        let reason = TransitionReason::StreamCompleted;
        let json = serde_json::to_string(&reason).unwrap();
        let rt: TransitionReason = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, reason);
    }

    #[test]
    fn session_transition_round_trips_via_serde() {
        let t = SessionTransition {
            from: SessionStatus::Running,
            to: SessionStatus::Completed,
            reason: TransitionReason::StreamCompleted,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&t).unwrap();
        let rt: SessionTransition = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.from, t.from);
        assert_eq!(rt.to, t.to);
        assert_eq!(rt.reason, t.reason);
    }
}
