//! Integration tests for `InteractionSession::send_turn` (#737, rewired onto
//! the `AgentProvider` seam in #751). No real process: a `ScriptedProvider`
//! pops one scripted turn per `run` call and records every `AgentRequest`,
//! so the resume chain is asserted at the transport-agnostic boundary.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::interaction::{InteractionSession, InteractionState, TurnRole};
use crate::session::interaction_turn::{TurnError, TurnEvent};
use crate::session::types::StreamEvent;

fn chunk(text: &str) -> StreamEvent {
    StreamEvent::AssistantMessage {
        text: text.to_string(),
    }
}

fn ok_turn(events: Vec<StreamEvent>, session_id: Option<&'static str>) -> ScriptedTurn {
    ScriptedTurn {
        events,
        end: ScriptedEnd::Ok {
            exit_code: Some(0),
            session_id,
        },
    }
}

fn make_session() -> InteractionSession {
    InteractionSession::new(1, PathBuf::from("/tmp/wt"), "feat/issue-1".into(), false)
}

fn drain(rx: &mut mpsc::Receiver<TurnEvent>) -> Vec<TurnEvent> {
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    events
}

#[tokio::test]
async fn first_turn_request_has_no_resume_and_carries_worktree_cwd() {
    let mut session = make_session();
    let (tx, _rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ok_turn(
        vec![chunk("hi")],
        Some("abc123"),
    )]));

    session
        .send_turn(
            "hello".into(),
            "claude-opus",
            provider.clone(),
            tx,
            CancellationToken::new(),
        )
        .await
        .expect("turn must succeed");

    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].prompt, "hello");
    assert_eq!(requests[0].model, "claude-opus");
    assert_eq!(
        requests[0].cwd.as_deref(),
        Some(std::path::Path::new("/tmp/wt"))
    );
    assert_eq!(
        requests[0].resume_session_id, None,
        "first turn must start a fresh conversation"
    );
}

#[tokio::test]
async fn subsequent_turn_request_includes_resume_session_id() {
    let mut session = make_session();
    let provider = Arc::new(ScriptedProvider::new(vec![
        ok_turn(vec![chunk("first")], Some("abc123")),
        ok_turn(vec![chunk("second")], Some("abc123")),
    ]));

    let (tx1, _rx1) = mpsc::channel(32);
    session
        .send_turn(
            "p1".into(),
            "m",
            provider.clone(),
            tx1,
            CancellationToken::new(),
        )
        .await
        .expect("turn 1");
    let (tx2, _rx2) = mpsc::channel(32);
    session
        .send_turn(
            "p2".into(),
            "m",
            provider.clone(),
            tx2,
            CancellationToken::new(),
        )
        .await
        .expect("turn 2");

    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].resume_session_id.as_deref(),
        Some("abc123"),
        "second turn must resume the bound session id"
    );
}

#[tokio::test]
async fn session_id_from_run_result_persisted_in_memory() {
    let mut session = make_session();
    let (tx, _rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ok_turn(vec![], Some("abc123"))]));

    session
        .send_turn("p".into(), "m", provider, tx, CancellationToken::new())
        .await
        .expect("turn");

    assert_eq!(session.session_id.as_deref(), Some("abc123"));
    assert_eq!(session.state, InteractionState::Idle);
}

#[tokio::test]
async fn stream_chunks_flow_through_events_tx_in_order() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ok_turn(
        vec![chunk("Hello, "), chunk("world.")],
        Some("abc123"),
    )]));

    session
        .send_turn("p".into(), "m", provider, tx, CancellationToken::new())
        .await
        .expect("turn");

    let events = drain(&mut rx);
    let chunks: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::Chunk(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(chunks, vec!["Hello, ", "world."]);
    assert!(
        matches!(events.last(), Some(TurnEvent::TurnFinished { .. })),
        "stream must end with TurnFinished: {events:?}"
    );

    let agent_turn = session
        .history
        .iter()
        .find(|t| t.role == TurnRole::Agent)
        .expect("agent turn recorded");
    assert_eq!(agent_turn.content, "Hello, world.");
}

#[tokio::test]
async fn turn_record_finished_at_set_after_run() {
    let mut session = make_session();
    let (tx, _rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ok_turn(
        vec![chunk("x")],
        Some("abc123"),
    )]));

    session
        .send_turn("p".into(), "m", provider, tx, CancellationToken::new())
        .await
        .expect("turn");

    let agent_turn = session
        .history
        .iter()
        .find(|t| t.role == TurnRole::Agent)
        .expect("agent turn");
    assert!(agent_turn.finished_at.is_some());
}

#[tokio::test]
async fn spawn_failure_sends_error_event_no_panic() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::SpawnFail,
    }]));

    let result = session
        .send_turn("p".into(), "m", provider, tx, CancellationToken::new())
        .await;

    assert!(matches!(result, Err(TurnError::Spawn(_))));
    assert_eq!(session.state, InteractionState::Idle);
    let events = drain(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TurnEvent::Error(msg) if msg.contains("spawn"))),
        "an Error event must surface the spawn failure: {events:?}"
    );
}

#[tokio::test]
async fn failed_status_emits_error_event_and_system_marker() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::FailedStatus {
            status: "2",
            stderr: "boom",
        },
    }]));

    let result = session
        .send_turn("p".into(), "m", provider, tx, CancellationToken::new())
        .await;

    match result {
        Err(TurnError::NonZeroExit { code, stderr_tail }) => {
            assert_eq!(code, Some(2));
            assert_eq!(stderr_tail, "boom");
        }
        other => panic!("expected NonZeroExit, got: {other:?}"),
    }
    let events = drain(&mut rx);
    assert!(
        events.iter().any(|e| matches!(e, TurnEvent::Error(_))),
        "Error event expected: {events:?}"
    );
    assert!(
        session
            .history
            .iter()
            .any(|t| t.role == TurnRole::System && t.content.contains("agent exit")),
        "system marker expected: {:?}",
        session.history
    );
}

#[tokio::test]
async fn cancellation_appends_system_cancel_turn() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
        events: vec![chunk("partial")],
        end: ScriptedEnd::WaitForCancel,
    }]));

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        trigger.cancel();
    });

    let result = session
        .send_turn("p".into(), "m", provider, tx, cancel)
        .await;

    assert!(result.is_ok(), "cancellation is not an error: {result:?}");
    assert_eq!(session.state, InteractionState::Idle);
    assert!(
        session
            .history
            .iter()
            .any(|t| t.role == TurnRole::System && t.content.contains("cancelled")),
        "system cancel marker expected: {:?}",
        session.history
    );
    let events = drain(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TurnEvent::Error(msg) if msg.contains("cancelled"))),
        "cancel notice expected: {events:?}"
    );
}

#[tokio::test]
async fn degraded_no_session_id_warns_and_next_turn_is_first_turn() {
    let mut session = make_session();
    let provider = Arc::new(ScriptedProvider::new(vec![
        ok_turn(vec![chunk("no id this time")], None),
        ok_turn(vec![chunk("still none")], None),
    ]));

    let (tx1, _rx1) = mpsc::channel(32);
    session
        .send_turn(
            "p1".into(),
            "m",
            provider.clone(),
            tx1,
            CancellationToken::new(),
        )
        .await
        .expect("turn 1");

    assert!(session.session_id.is_none());
    assert!(
        session
            .history
            .iter()
            .any(|t| t.role == TurnRole::System && t.content.contains("could not bind")),
        "degraded-mode marker expected: {:?}",
        session.history
    );

    let (tx2, _rx2) = mpsc::channel(32);
    session
        .send_turn(
            "p2".into(),
            "m",
            provider.clone(),
            tx2,
            CancellationToken::new(),
        )
        .await
        .expect("turn 2");

    let requests = provider.requests.lock().unwrap();
    assert_eq!(
        requests[1].resume_session_id, None,
        "without a bound id the next turn must re-init (no resume)"
    );
}
