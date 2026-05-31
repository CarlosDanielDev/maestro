//! Integration tests for `InteractionSession::send_turn` (#737). No real
//! process is spawned — a `FakeSpawner` feeds canned stream-json + exit outcome.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::session::interaction::{InteractionSession, TurnRole};
use crate::session::interaction_turn::{
    SpawnAgent, SpawnError, TurnArgv, TurnEvent, TurnHandle, TurnOutcome,
};

const SYS_WITH_SESSION_ID: &str =
    r#"{"type":"system","subtype":"init","session_id":"abc123","tools":[]}"#;
const SYS_NO_SESSION_ID: &str = r#"{"type":"system","subtype":"init","tools":[]}"#;
const ASSISTANT_CHUNK_1: &str =
    r#"{"type":"assistant","message":{"type":"text","text":"Hello, "}}"#;
const ASSISTANT_CHUNK_2: &str = r#"{"type":"assistant","message":{"type":"text","text":"world."}}"#;
const RESULT_EVENT: &str =
    r#"{"type":"result","subtype":"success","cost_usd":0.01,"session_id":"abc123"}"#;
/// Degraded fixture: neither the system nor the result line carries a
/// `session_id`, so the turn must fall into degraded re-init mode.
const RESULT_NO_SESSION_ID: &str = r#"{"type":"result","subtype":"success","cost_usd":0.01}"#;

/// One canned turn: the stdout lines, plus the process exit outcome.
struct Batch {
    lines: Vec<&'static str>,
    outcome: TurnOutcome,
}

impl Batch {
    fn ok(lines: Vec<&'static str>) -> Self {
        Self {
            lines,
            outcome: TurnOutcome {
                exit_code: Some(0),
                stderr_tail: String::new(),
            },
        }
    }

    fn exit(lines: Vec<&'static str>, code: i32, stderr: &str) -> Self {
        Self {
            lines,
            outcome: TurnOutcome {
                exit_code: Some(code),
                stderr_tail: stderr.to_string(),
            },
        }
    }
}

/// Real fake `SpawnAgent`: pops a `Batch` per spawn, streams its lines into the
/// channel, then resolves the outcome. Records every argv it was called with.
struct FakeSpawner {
    batches: Mutex<VecDeque<Batch>>,
    calls: Arc<Mutex<Vec<TurnArgv>>>,
    fail_next: bool,
}

impl FakeSpawner {
    fn new(batches: Vec<Batch>) -> Self {
        Self {
            batches: Mutex::new(batches.into()),
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_next: false,
        }
    }

    fn failing() -> Self {
        let mut s = Self::new(vec![]);
        s.fail_next = true;
        s
    }
}

#[async_trait::async_trait]
impl SpawnAgent for FakeSpawner {
    async fn spawn(
        &self,
        argv: TurnArgv,
        _cancel: CancellationToken,
    ) -> Result<TurnHandle, SpawnError> {
        self.calls.lock().unwrap().push(argv);
        if self.fail_next {
            return Err(SpawnError::Other("injected failure".into()));
        }
        let batch = self
            .batches
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Batch::ok(vec![]));

        let (lines_tx, lines_rx) = mpsc::channel::<String>(64);
        let (outcome_tx, outcome_rx) = oneshot::channel::<TurnOutcome>();

        tokio::spawn(async move {
            for line in batch.lines {
                if lines_tx.send(line.to_string()).await.is_err() {
                    break;
                }
            }
            drop(lines_tx);
            let _ = outcome_tx.send(batch.outcome);
        });

        Ok(TurnHandle {
            lines: lines_rx,
            outcome: outcome_rx,
        })
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
async fn first_turn_argv_has_expected_flags() {
    let mut session = make_session();
    let (tx, _rx) = mpsc::channel(32);
    let spawner = FakeSpawner::new(vec![Batch::ok(vec![SYS_WITH_SESSION_ID, RESULT_EVENT])]);

    session
        .send_turn(
            "fix the bug".into(),
            "claude-opus-4-8",
            &spawner,
            tx,
            CancellationToken::new(),
        )
        .await
        .unwrap();

    let calls = spawner.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let argv = &calls[0].args;
    assert!(argv.contains(&"--print".to_string()));
    assert!(argv.contains(&"--verbose".to_string()));
    assert!(
        argv.windows(2)
            .any(|w| w == ["--output-format", "stream-json"])
    );
    assert!(argv.windows(2).any(|w| w == ["--model", "claude-opus-4-8"]));
    assert!(argv.windows(2).any(|w| w == ["-p", "fix the bug"]));
    assert!(
        !argv.contains(&"--resume".to_string()),
        "first turn must not resume"
    );
    assert_eq!(calls[0].cwd, PathBuf::from("/tmp/wt"));
}

#[tokio::test]
async fn subsequent_turn_argv_includes_resume_with_session_id() {
    let mut session = make_session();
    let spawner = FakeSpawner::new(vec![
        Batch::ok(vec![SYS_WITH_SESSION_ID, RESULT_EVENT]),
        Batch::ok(vec![SYS_WITH_SESSION_ID, RESULT_EVENT]),
    ]);

    let (tx1, _r1) = mpsc::channel(32);
    session
        .send_turn("p1".into(), "m", &spawner, tx1, CancellationToken::new())
        .await
        .unwrap();
    let (tx2, _r2) = mpsc::channel(32);
    session
        .send_turn("p2".into(), "m", &spawner, tx2, CancellationToken::new())
        .await
        .unwrap();

    let calls = spawner.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let argv2 = &calls[1].args;
    let pos = argv2
        .iter()
        .position(|a| a == "--resume")
        .expect("second turn must include --resume");
    assert_eq!(argv2[pos + 1], "abc123");
    assert!(
        argv2.contains(&"--verbose".to_string()),
        "resume turn must keep --verbose: claude CLI rejects --output-format=stream-json without it"
    );
}

#[tokio::test]
async fn first_system_event_session_id_persisted_in_memory() {
    let mut session = make_session();
    assert!(session.session_id.is_none());
    let (tx, _rx) = mpsc::channel(32);
    let spawner = FakeSpawner::new(vec![Batch::ok(vec![SYS_WITH_SESSION_ID, RESULT_EVENT])]);

    session
        .send_turn("p".into(), "m", &spawner, tx, CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(session.session_id, Some("abc123".to_string()));
}

#[tokio::test]
async fn stream_chunks_flow_through_events_tx_in_order() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let spawner = FakeSpawner::new(vec![Batch::ok(vec![
        SYS_WITH_SESSION_ID,
        ASSISTANT_CHUNK_1,
        ASSISTANT_CHUNK_2,
        RESULT_EVENT,
    ])]);

    session
        .send_turn("p".into(), "m", &spawner, tx, CancellationToken::new())
        .await
        .unwrap();

    let events = drain(&mut rx);
    assert!(matches!(
        &events[0],
        TurnEvent::TurnStarted {
            role: TurnRole::Agent,
            ..
        }
    ));
    assert!(matches!(&events[1], TurnEvent::Chunk(s) if s == "Hello, "));
    assert!(matches!(&events[2], TurnEvent::Chunk(s) if s == "world."));
    assert!(matches!(
        events.last(),
        Some(TurnEvent::TurnFinished { .. })
    ));
}

#[tokio::test]
async fn turn_record_finished_at_set_on_result_event() {
    let mut session = make_session();
    let (tx, _rx) = mpsc::channel(32);
    let spawner = FakeSpawner::new(vec![Batch::ok(vec![
        SYS_WITH_SESSION_ID,
        ASSISTANT_CHUNK_1,
        RESULT_EVENT,
    ])]);

    session
        .send_turn(
            "my prompt".into(),
            "m",
            &spawner,
            tx,
            CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(session.history.len(), 2, "user + agent turns");
    assert_eq!(session.history[0].role, TurnRole::User);
    assert_eq!(session.history[0].content, "my prompt");
    let agent = &session.history[1];
    assert_eq!(agent.role, TurnRole::Agent);
    assert!(agent.finished_at.is_some());
    assert!(agent.content.contains("Hello, "));
    assert_eq!(
        session.state,
        crate::session::interaction::InteractionState::Idle
    );
}

#[tokio::test]
async fn spawn_failure_sends_error_event_no_panic() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let spawner = FakeSpawner::failing();

    let result = session
        .send_turn("p".into(), "m", &spawner, tx, CancellationToken::new())
        .await;

    assert!(matches!(
        result,
        Err(crate::session::interaction_turn::TurnError::Spawn(_))
    ));
    let events = drain(&mut rx);
    assert!(matches!(events.last(), Some(TurnEvent::Error(_))));
    assert_eq!(
        session.state,
        crate::session::interaction::InteractionState::Idle
    );
}

#[tokio::test]
async fn nonzero_exit_emits_error_event() {
    let mut session = make_session();
    let (tx, mut rx) = mpsc::channel(32);
    let spawner = FakeSpawner::new(vec![Batch::exit(vec![SYS_WITH_SESSION_ID], 2, "boom")]);

    let result = session
        .send_turn("p".into(), "m", &spawner, tx, CancellationToken::new())
        .await;

    assert!(matches!(
        result,
        Err(crate::session::interaction_turn::TurnError::NonZeroExit { code: Some(2), .. })
    ));
    let events = drain(&mut rx);
    assert!(events.iter().any(
        |e| matches!(e, TurnEvent::Error(m) if m.contains("agent exit 2") && m.contains("boom"))
    ));
    assert_eq!(
        session.state,
        crate::session::interaction::InteractionState::Idle
    );
}

#[tokio::test]
async fn cancellation_appends_system_cancel_turn() {
    let mut session = make_session();
    let (tx, _rx) = mpsc::channel(32);
    let cancel = CancellationToken::new();
    cancel.cancel(); // pre-cancelled
    let spawner = FakeSpawner::new(vec![Batch::ok(vec![
        SYS_WITH_SESSION_ID,
        ASSISTANT_CHUNK_1,
    ])]);

    let result = session
        .send_turn("p".into(), "m", &spawner, tx, cancel)
        .await;

    assert!(result.is_ok(), "cancellation is a clean exit");
    let sys: Vec<_> = session
        .history
        .iter()
        .filter(|t| t.role == TurnRole::System)
        .collect();
    assert!(!sys.is_empty(), "a System cancel turn must be appended");
    assert!(
        sys.iter()
            .any(|t| t.content.to_lowercase().contains("cancel"))
    );
    assert_eq!(
        session.state,
        crate::session::interaction::InteractionState::Idle
    );
}

#[tokio::test]
async fn degraded_no_session_id_warns_and_next_turn_is_first_turn() {
    let mut session = make_session();
    let spawner = FakeSpawner::new(vec![
        Batch::ok(vec![
            SYS_NO_SESSION_ID,
            ASSISTANT_CHUNK_1,
            RESULT_NO_SESSION_ID,
        ]),
        Batch::ok(vec![SYS_NO_SESSION_ID, RESULT_NO_SESSION_ID]),
    ]);

    let (tx1, _r1) = mpsc::channel(32);
    session
        .send_turn("p1".into(), "m", &spawner, tx1, CancellationToken::new())
        .await
        .unwrap();

    assert!(session.session_id.is_none(), "degraded: no session_id");
    let sys: Vec<_> = session
        .history
        .iter()
        .filter(|t| t.role == TurnRole::System)
        .collect();
    assert!(
        sys.iter().any(|t| t.content.contains("session_id")),
        "degraded warning System turn must mention session_id"
    );

    let (tx2, _r2) = mpsc::channel(32);
    session
        .send_turn("p2".into(), "m", &spawner, tx2, CancellationToken::new())
        .await
        .unwrap();

    let calls = spawner.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        !calls[1].args.contains(&"--resume".to_string()),
        "degraded: turn 2 is a fresh first turn"
    );
}
