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

// --- Issue #739: signal_terminator ---

use super::super::interaction_lifecycle::InteractionLifecycleEvent;

fn pr_linked(pr_number: u64, issue_number: u64) -> InteractionLifecycleEvent {
    InteractionLifecycleEvent::PrLinkedToIssue {
        pr_number,
        issue_number,
        owner: "owner".into(),
        repo: "repo".into(),
    }
}

#[test]
fn signal_terminator_idle_fires_immediately() {
    let before = Utc::now();
    let mut s = make_interaction(42);
    s.signal_terminator(pr_linked(7, 42));
    let after = Utc::now();

    assert_eq!(s.state, InteractionState::Terminated);
    assert_eq!(
        s.close_reason,
        Some(CloseReason::PrCreated { pr_number: 7 })
    );
    let closed = s.closed_at.expect("closed_at must be set");
    assert!(closed >= before && closed <= after);
    assert!(s.queued_terminator.is_none());
}

#[test]
fn signal_terminator_streaming_queues_and_stays_streaming() {
    let mut s = make_interaction(42);
    s.state = InteractionState::Streaming;

    s.signal_terminator(pr_linked(99, 42));

    assert_eq!(s.state, InteractionState::Streaming);
    assert!(s.close_reason.is_none());
    assert!(s.closed_at.is_none());
    assert!(matches!(
        s.queued_terminator.as_ref().unwrap(),
        InteractionLifecycleEvent::PrLinkedToIssue { pr_number: 99, .. }
    ));
}

#[test]
fn signal_terminator_terminated_is_noop() {
    let mut s = make_interaction(1);
    s.state = InteractionState::Terminated;
    s.close_reason = Some(CloseReason::UserQuit);
    s.closed_at = Some(Utc::now());

    s.signal_terminator(pr_linked(5, 1));

    assert_eq!(s.state, InteractionState::Terminated);
    assert_eq!(s.close_reason, Some(CloseReason::UserQuit));
    assert!(s.queued_terminator.is_none());
}

#[test]
fn signal_terminator_double_call_second_call_is_noop() {
    let mut s = make_interaction(1);
    s.signal_terminator(pr_linked(10, 1));
    s.signal_terminator(pr_linked(20, 1));

    assert_eq!(s.state, InteractionState::Terminated);
    assert_eq!(
        s.close_reason,
        Some(CloseReason::PrCreated { pr_number: 10 })
    );
}

#[test]
fn interaction_session_serde_roundtrip_queued_terminator_is_skipped() {
    let mut s = make_interaction(3);
    s.state = InteractionState::Streaming;
    s.signal_terminator(pr_linked(55, 3));
    assert!(s.queued_terminator.is_some(), "precondition: event queued");

    let json = serde_json::to_string(&s).unwrap();
    let rt: InteractionSession = serde_json::from_str(&json).unwrap();

    assert!(rt.queued_terminator.is_none());
    assert_eq!(rt.state, InteractionState::Streaming);
}
