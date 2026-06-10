//! Integration tests for the live interaction turn → screen path (#738).
//!
//! Drives `InteractionSession::send_turn` with a `ScriptedProvider` (#751),
//! then threads the emitted `TurnEvent`s through
//! `InteractionScreen::apply_turn_event` exactly as the TUI command pump does
//! — verifying the event shapes from #737 produce the right live transcript +
//! activity-log line.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::interaction::{InteractionSession, InteractionState, TurnRole};
use crate::session::interaction_turn::TurnEvent;
use crate::session::types::StreamEvent;
use crate::tui::screens::{InteractionScreen, ScreenAction};

fn chunk(text: &str) -> StreamEvent {
    StreamEvent::AssistantMessage {
        text: text.to_string(),
    }
}

fn session() -> InteractionSession {
    InteractionSession::new(13, PathBuf::from("/tmp/wt"), "feat/issue-13".into(), false)
}

/// Collect every event `send_turn` emits for one turn.
async fn run_turn(
    session: &mut InteractionSession,
    provider: Arc<ScriptedProvider>,
) -> Vec<TurnEvent> {
    let (tx, mut rx) = mpsc::channel(64);
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    });
    let _ = session
        .send_turn("hi".into(), "opus", provider, tx, CancellationToken::new())
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

    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
        events: vec![chunk("Hello, "), chunk("world.")],
        end: ScriptedEnd::Ok {
            exit_code: Some(0),
            session_id: Some("abc123"),
        },
    }]));
    let events = run_turn(&mut sess, provider).await;

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

    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::FailedStatus {
            status: "1",
            stderr: "boom",
        },
    }]));
    let events = run_turn(&mut sess, provider).await;

    for ev in &events {
        screen.apply_turn_event(ev);
    }
    assert_eq!(screen.history_state(), InteractionState::Idle);
    assert!(
        screen.last_is_system_error(),
        "a non-zero exit must append a System turn to the screen"
    );
}
