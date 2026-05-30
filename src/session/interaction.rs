//! Interactive (chat-style) session scaffold (#734).
//!
//! Type plumbing only — no spawn or UI behavior. The per-turn
//! `claude --resume` loop lands in #737; the Interaction screen in #736.
//! `InteractionState` models the long-lived conversation lifecycle and is
//! deliberately separate from `super::types::SessionStatus`, which models
//! one-shot work.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Lifecycle phase of an interactive session. `#737` drives
/// `Idle` ↔ `Streaming`; `Terminated` is set once on close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionState {
    #[default]
    Idle,
    Streaming,
    Terminated,
}

/// Author of a single conversation turn. Named `TurnRole` to avoid
/// collision with `super::role::Role` (agent personality), which is a
/// distinct concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Agent,
    System,
}

/// Why an interaction session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    PrCreated { pr_number: u64 },
    UserQuit,
    AgentFailure { tail: String },
}

/// One conversational turn. `finished_at` is `None` while a turn streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub role: TurnRole,
    pub content: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

/// Persisted state for one interactive session attached to an issue.
/// Scaffold only (#734): #736 renders it, #737 drives turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionSession {
    pub issue_number: u64,
    pub worktree_path: PathBuf,
    pub branch: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub state: InteractionState,
    #[serde(default)]
    pub history: Vec<TurnRecord>,
    pub produce_pr: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub close_reason: Option<CloseReason>,
}

impl InteractionSession {
    /// Construct a fresh, open session. `created_at` is stamped now and
    /// `state` starts at `Idle`.
    #[allow(dead_code)] // Reason: scaffold for #736/#737 — constructed by the spawn loop
    pub fn new(
        issue_number: u64,
        worktree_path: PathBuf,
        branch: String,
        produce_pr: bool,
    ) -> Self {
        Self {
            issue_number,
            worktree_path,
            branch,
            session_id: None,
            state: InteractionState::Idle,
            history: Vec::new(),
            produce_pr,
            created_at: Utc::now(),
            closed_at: None,
            close_reason: None,
        }
    }

    /// True while the session has not been terminated. Callers ask this
    /// instead of matching on `state` directly.
    pub fn is_active(&self) -> bool {
        self.state != InteractionState::Terminated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_interaction(issue: u64) -> InteractionSession {
        InteractionSession::new(
            issue,
            PathBuf::from("/tmp/test-wt"),
            format!("feat/issue-{issue}"),
            false,
        )
    }

    // --- InteractionState ---

    #[test]
    fn interaction_state_default_is_idle() {
        assert_eq!(InteractionState::default(), InteractionState::Idle);
    }

    #[test]
    fn interaction_state_idle_serializes_as_snake_case() {
        let json = serde_json::to_string(&InteractionState::Idle).unwrap();
        assert_eq!(json, r#""idle""#);
    }

    #[test]
    fn interaction_state_streaming_serializes_as_snake_case() {
        let json = serde_json::to_string(&InteractionState::Streaming).unwrap();
        assert_eq!(json, r#""streaming""#);
    }

    #[test]
    fn interaction_state_terminated_serializes_as_snake_case() {
        let json = serde_json::to_string(&InteractionState::Terminated).unwrap();
        assert_eq!(json, r#""terminated""#);
    }

    #[test]
    fn interaction_state_round_trips_via_serde() {
        for state in [
            InteractionState::Idle,
            InteractionState::Streaming,
            InteractionState::Terminated,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let rt: InteractionState = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, state);
        }
    }

    // --- TurnRole ---

    #[test]
    fn turn_role_user_serializes_as_snake_case() {
        let json = serde_json::to_string(&TurnRole::User).unwrap();
        assert_eq!(json, r#""user""#);
    }

    #[test]
    fn turn_role_agent_serializes_as_snake_case() {
        let json = serde_json::to_string(&TurnRole::Agent).unwrap();
        assert_eq!(json, r#""agent""#);
    }

    #[test]
    fn turn_role_system_serializes_as_snake_case() {
        let json = serde_json::to_string(&TurnRole::System).unwrap();
        assert_eq!(json, r#""system""#);
    }

    #[test]
    fn turn_role_round_trips_via_serde() {
        for role in [TurnRole::User, TurnRole::Agent, TurnRole::System] {
            let json = serde_json::to_string(&role).unwrap();
            let rt: TurnRole = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, role);
        }
    }

    // --- CloseReason ---

    #[test]
    fn close_reason_pr_created_serializes_pr_number() {
        let json = serde_json::to_string(&CloseReason::PrCreated { pr_number: 42 }).unwrap();
        assert!(json.contains(r#""pr_number":42"#), "got: {json}");
    }

    #[test]
    fn close_reason_user_quit_serializes_as_snake_case() {
        let json = serde_json::to_string(&CloseReason::UserQuit).unwrap();
        assert!(json.contains("user_quit"), "got: {json}");
    }

    #[test]
    fn close_reason_agent_failure_serializes_tail() {
        let json = serde_json::to_string(&CloseReason::AgentFailure {
            tail: "oom".to_string(),
        })
        .unwrap();
        assert!(json.contains(r#""tail":"oom""#), "got: {json}");
    }

    #[test]
    fn close_reason_round_trips_via_serde() {
        let reasons = [
            CloseReason::PrCreated { pr_number: 7 },
            CloseReason::UserQuit,
            CloseReason::AgentFailure { tail: "err".into() },
        ];
        for reason in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            let rt: CloseReason = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, reason);
        }
    }

    // --- TurnRecord ---

    #[test]
    fn turn_record_finished_at_defaults_to_none_via_serde() {
        let json = r#"{"role":"user","content":"hi","started_at":"2026-05-30T00:00:00Z"}"#;
        let rt: TurnRecord = serde_json::from_str(json).unwrap();
        assert!(rt.finished_at.is_none());
    }

    #[test]
    fn turn_record_round_trips_via_serde() {
        let record = TurnRecord {
            role: TurnRole::User,
            content: "hello".into(),
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
        };
        let json = serde_json::to_string(&record).unwrap();
        let rt: TurnRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.role, TurnRole::User);
        assert_eq!(rt.content, "hello");
        assert!(rt.finished_at.is_some());
    }

    // --- InteractionSession ---

    #[test]
    fn interaction_session_new_stamps_created_at_and_state_is_idle() {
        let before = Utc::now();
        let s = InteractionSession::new(42, PathBuf::from("/tmp/wt"), "feat/x".into(), true);
        let after = Utc::now();
        assert_eq!(s.issue_number, 42);
        assert_eq!(s.worktree_path, PathBuf::from("/tmp/wt"));
        assert_eq!(s.branch, "feat/x");
        assert!(s.produce_pr);
        assert_eq!(s.state, InteractionState::Idle);
        assert!(s.history.is_empty());
        assert!(s.session_id.is_none());
        assert!(s.closed_at.is_none());
        assert!(s.close_reason.is_none());
        assert!(s.created_at >= before && s.created_at <= after);
    }

    #[test]
    fn interaction_session_is_active_when_idle() {
        let s = make_interaction(1);
        assert!(s.is_active());
    }

    #[test]
    fn interaction_session_is_active_when_streaming() {
        let mut s = make_interaction(1);
        s.state = InteractionState::Streaming;
        assert!(s.is_active());
    }

    #[test]
    fn interaction_session_is_not_active_when_terminated() {
        let mut s = make_interaction(1);
        s.state = InteractionState::Terminated;
        assert!(!s.is_active());
    }

    #[test]
    fn interaction_session_round_trips_via_serde() {
        let mut s = InteractionSession::new(7, PathBuf::from("/tmp/w"), "main".into(), false);
        s.history.push(TurnRecord {
            role: TurnRole::Agent,
            content: "done".into(),
            started_at: Utc::now(),
            finished_at: None,
        });
        let json = serde_json::to_string(&s).unwrap();
        let rt: InteractionSession = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.issue_number, 7);
        assert!(!rt.produce_pr);
        assert_eq!(rt.history.len(), 1);
    }

    #[test]
    fn interaction_session_closed_at_deserializes_with_default_when_absent() {
        let s = make_interaction(3);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","closed_at":null"#, "");
        let rt: InteractionSession = serde_json::from_str(&stripped).unwrap();
        assert!(rt.closed_at.is_none());
    }

    #[test]
    fn interaction_session_close_reason_deserializes_with_default_when_absent() {
        let s = make_interaction(3);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","close_reason":null"#, "");
        let rt: InteractionSession = serde_json::from_str(&stripped).unwrap();
        assert!(rt.close_reason.is_none());
    }

    #[test]
    fn interaction_session_session_id_deserializes_with_default_when_absent() {
        let s = make_interaction(3);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","session_id":null"#, "");
        let rt: InteractionSession = serde_json::from_str(&stripped).unwrap();
        assert!(rt.session_id.is_none());
    }

    #[test]
    fn interaction_session_history_deserializes_with_default_when_absent() {
        let s = make_interaction(3);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","history":[]"#, "");
        let rt: InteractionSession = serde_json::from_str(&stripped).unwrap();
        assert!(rt.history.is_empty());
    }
}
