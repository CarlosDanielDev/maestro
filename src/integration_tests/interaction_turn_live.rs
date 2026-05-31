//! Integration tests for the live interaction turn → screen path (#738).
//!
//! Drives `InteractionSession::send_turn` with a canned `SpawnAgent`, then
//! threads the emitted `TurnEvent`s through `InteractionScreen::apply_turn_event`
//! exactly as the TUI command pump does — verifying the event shapes from #737
//! produce the right live transcript + activity-log line.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::session::interaction::{InteractionSession, InteractionState, TurnRole};
use crate::session::interaction_turn::{
    SpawnAgent, SpawnError, TurnArgv, TurnEvent, TurnHandle, TurnOutcome,
};
use crate::tui::screens::{InteractionScreen, ScreenAction};

const SYS: &str = r#"{"type":"system","subtype":"init","session_id":"abc123","tools":[]}"#;
const CHUNK_1: &str = r#"{"type":"assistant","message":{"type":"text","text":"Hello, "}}"#;
const CHUNK_2: &str = r#"{"type":"assistant","message":{"type":"text","text":"world."}}"#;
const RESULT: &str =
    r#"{"type":"result","subtype":"success","cost_usd":0.01,"session_id":"abc123"}"#;

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

struct FakeSpawner {
    batches: Mutex<VecDeque<Batch>>,
}

impl FakeSpawner {
    fn new(batches: Vec<Batch>) -> Self {
        Self {
            batches: Mutex::new(batches.into()),
        }
    }
}

#[async_trait::async_trait]
impl SpawnAgent for FakeSpawner {
    async fn spawn(
        &self,
        _argv: TurnArgv,
        _cancel: CancellationToken,
    ) -> Result<TurnHandle, SpawnError> {
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

fn session() -> InteractionSession {
    InteractionSession::new(13, PathBuf::from("/tmp/wt"), "feat/issue-13".into(), false)
}

/// Collect every event `send_turn` emits for one turn.
async fn run_turn(session: &mut InteractionSession, spawner: &dyn SpawnAgent) -> Vec<TurnEvent> {
    let (tx, mut rx) = mpsc::channel(64);
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    });
    let _ = session
        .send_turn("hi".into(), "opus", spawner, tx, CancellationToken::new())
        .await;
    collector.await.unwrap()
}

#[tokio::test]
async fn live_turn_streams_into_screen_and_logs_chunk_count() {
    let mut sess = session();
    let mut screen = InteractionScreen::for_session(&sess);
    // The screen pushes the user turn + flips Streaming on send; mirror that.
    screen.push_turn(crate::session::interaction::TurnRecord {
        role: TurnRole::User,
        content: "hi".into(),
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    });

    let spawner = FakeSpawner::new(vec![Batch::ok(vec![SYS, CHUNK_1, CHUNK_2, RESULT])]);
    let events = run_turn(&mut sess, &spawner).await;

    let mut last_log: Option<String> = None;
    for ev in &events {
        if let ScreenAction::LogActivity { tag, message, .. } = screen.apply_turn_event(ev) {
            assert_eq!(tag, "INTERACTION");
            last_log = Some(message);
        }
    }

    assert_eq!(screen.history_state(), InteractionState::Idle);
    // Agent turn accumulated both chunks.
    assert!(
        screen.last_agent_content().contains("Hello, world."),
        "agent content: {:?}",
        screen.last_agent_content()
    );
    let log = last_log.expect("TurnFinished must produce an activity-log line");
    assert!(log.contains("#13"), "got: {log}");
    assert!(log.contains("chunks streamed"), "got: {log}");
}

#[tokio::test]
async fn live_turn_error_appends_system_turn_to_screen() {
    let mut sess = session();
    let mut screen = InteractionScreen::for_session(&sess);

    let spawner = FakeSpawner::new(vec![Batch::exit(vec![SYS], 1, "boom")]);
    let events = run_turn(&mut sess, &spawner).await;

    for ev in &events {
        screen.apply_turn_event(ev);
    }
    assert_eq!(screen.history_state(), InteractionState::Idle);
    assert!(
        screen.last_is_system_error(),
        "a non-zero exit must append a System turn to the screen"
    );
}
