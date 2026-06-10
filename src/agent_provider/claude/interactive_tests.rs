//! Unit tests for the interactive PTY transport (issues #749/#751), using a
//! trait-based mock backend per RUST-GUARDRAILS.md §7 — no real `claude`
//! binary, no real PTY (except the stub-script e2e at the bottom).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::agent_provider::types::{AgentError, AgentProviderEvent, AgentRequest};
use crate::session::types::StreamEvent;

pub(super) const MODE_LINE: &str = r#"{"type":"mode","mode":"normal","sessionId":"s"}"#;
pub(super) const ASSISTANT_LINE: &str = r#"{"type":"assistant","message":{"id":"m1","type":"message","role":"assistant","model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"hi from mock"}],"stop_reason":"end_turn"}}"#;
pub(super) const TURN_DURATION_LINE: &str =
    r#"{"type":"system","subtype":"turn_duration","durationMs":42,"messageCount":2}"#;

pub(super) const FRESH_ID: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

#[derive(Clone, Default)]
pub(super) struct MockState {
    pub(super) writes: Arc<Mutex<Vec<String>>>,
    pub(super) killed: Arc<AtomicBool>,
    pub(super) spawns: Arc<AtomicUsize>,
    pub(super) spawn_args: Arc<Mutex<Vec<Vec<String>>>>,
}

pub(super) struct MockChild {
    pub(super) state: MockState,
    pub(super) transcript: PathBuf,
    pub(super) respond: bool,
    pub(super) exited: Option<i32>,
}

impl InteractiveChild for MockChild {
    fn write_text(&mut self, text: &str) -> Result<(), AgentError> {
        self.state.writes.lock().unwrap().push(text.to_string());
        if text.contains("\u{1b}[200~") && self.respond {
            let mut body = std::fs::read_to_string(&self.transcript).unwrap_or_default();
            body.push_str(ASSISTANT_LINE);
            body.push('\n');
            body.push_str(TURN_DURATION_LINE);
            body.push('\n');
            std::fs::write(&self.transcript, body).unwrap();
        }
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<i32>, AgentError> {
        if self.state.killed.load(Ordering::Relaxed) {
            return Ok(Some(-9));
        }
        Ok(self.exited)
    }

    fn kill(&mut self) -> Result<(), AgentError> {
        self.state.killed.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn process_id(&self) -> Option<u32> {
        Some(4242)
    }
}

pub(super) struct MockBackend {
    pub(super) state: MockState,
    pub(super) create_transcript: bool,
    pub(super) respond: bool,
    pub(super) spawn_delay: Option<Duration>,
    pub(super) transcript: PathBuf,
}

impl MockBackend {
    pub(super) fn new(transcript: PathBuf) -> Self {
        Self {
            state: MockState::default(),
            create_transcript: true,
            respond: true,
            spawn_delay: None,
            transcript,
        }
    }
}

impl InteractiveBackend for MockBackend {
    fn spawn(&self, spec: &SpawnSpec) -> Result<Box<dyn InteractiveChild>, AgentError> {
        self.state.spawns.fetch_add(1, Ordering::Relaxed);
        self.state
            .spawn_args
            .lock()
            .unwrap()
            .push(spec.args.clone());
        if let Some(delay) = self.spawn_delay {
            std::thread::sleep(delay);
        }
        if self.create_transcript {
            if let Some(parent) = self.transcript.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            if !self.transcript.exists() {
                std::fs::write(&self.transcript, format!("{MODE_LINE}\n")).unwrap();
            }
        }
        Ok(Box::new(MockChild {
            state: self.state.clone(),
            transcript: self.transcript.clone(),
            respond: self.respond,
            exited: None,
        }))
    }
}

pub(super) fn run_spec(home: &Path) -> RunSpec {
    RunSpec::new("claude", "claude", home.to_path_buf(), FRESH_ID)
}

pub(super) fn request(cwd: &Path) -> AgentRequest {
    let mut request = AgentRequest::stream_json("hello there".into(), "claude-haiku".into());
    request.cwd = Some(cwd.to_path_buf());
    request
}

pub(super) fn drain_events(
    rx: &mut mpsc::UnboundedReceiver<AgentProviderEvent>,
) -> Vec<AgentProviderEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

pub(super) fn empty_slots() -> SessionSlots {
    SessionSlots::default()
}

#[test]
fn scrub_list_covers_fixed_vars_and_prefix_sweep() {
    let names = vec![
        "PATH".to_string(),
        "ANTHROPIC_CUSTOM_THING".to_string(),
        "HOME".to_string(),
    ];
    let scrubbed = scrub_env_names(names.into_iter());
    assert!(scrubbed.contains(&"ANTHROPIC_API_KEY".to_string()));
    assert!(scrubbed.contains(&"ANTHROPIC_AUTH_TOKEN".to_string()));
    assert!(scrubbed.contains(&"ANTHROPIC_CUSTOM_THING".to_string()));
    assert!(!scrubbed.iter().any(|n| n == "PATH" || n == "HOME"));
}

#[test]
fn transcript_path_uses_claude_code_munge_rule() {
    let path = transcript_path(
        Path::new("/home/u"),
        Path::new("/Users/x/proj.dir"),
        "abc-123",
    );
    assert_eq!(
        path,
        PathBuf::from("/home/u/.claude/projects/-Users-x-proj-dir/abc-123.jsonl")
    );
}

#[tokio::test]
async fn fresh_turn_streams_events_and_parks_child() {
    let tmp = tempfile::tempdir().unwrap();
    let spec = run_spec(tmp.path());
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), FRESH_ID);
    let backend = Arc::new(MockBackend::new(transcript));
    let state = backend.state.clone();
    let slots = empty_slots();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = run_session_turn(
        backend,
        Arc::clone(&slots),
        spec,
        request,
        tx,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.session_id.as_deref(), Some(FRESH_ID));
    assert_eq!(result.exit_code, None, "parked child has no exit code");

    let events = drain_events(&mut rx);
    assert!(
        matches!(events.first(), Some(AgentProviderEvent::Started(s)) if s.process_id == Some(4242)),
        "first event must be Started: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentProviderEvent::Stream(StreamEvent::AssistantMessage { text }) if text == "hi from mock"
        )),
        "assistant text must flow through: {events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(AgentProviderEvent::Stream(StreamEvent::Completed { .. }))
        ),
        "last event must be Completed: {events:?}"
    );

    let writes: Vec<String> = state.writes.lock().unwrap().clone();
    assert!(
        writes[0].starts_with("\u{1b}[200~") && writes[0].ends_with("\u{1b}[201~\r"),
        "prompt must be bracketed-paste wrapped: {writes:?}"
    );
    assert!(
        !writes.iter().any(|w| w.starts_with("/exit")),
        "the child is parked between turns — no /exit: {writes:?}"
    );
    assert!(!state.killed.load(Ordering::Relaxed), "park must not kill");
    assert!(
        slots.lock().await.contains_key(FRESH_ID),
        "child must be parked under the bound session id"
    );
    let spawn_args: Vec<Vec<String>> = state.spawn_args.lock().unwrap().clone();
    assert!(
        spawn_args[0]
            .windows(2)
            .any(|w| w == ["--session-id", FRESH_ID]),
        "fresh spawn pins --session-id: {spawn_args:?}"
    );
}

#[tokio::test]
async fn second_turn_reuses_parked_child_without_new_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let transcript = transcript_path(tmp.path(), tmp.path(), FRESH_ID);
    let backend = Arc::new(MockBackend::new(transcript));
    let state = backend.state.clone();
    let slots = empty_slots();

    // Turn 1: fresh.
    let (tx1, _rx1) = mpsc::unbounded_channel();
    let result = run_session_turn(
        Arc::clone(&backend) as Arc<dyn InteractiveBackend>,
        Arc::clone(&slots),
        run_spec(tmp.path()),
        request(tmp.path()),
        tx1,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    // Turn 2: resume the bound id.
    let mut second = request(tmp.path());
    second.resume_session_id = result.session_id.clone();
    let (tx2, mut rx2) = mpsc::unbounded_channel();
    let result2 = run_session_turn(
        backend,
        Arc::clone(&slots),
        run_spec(tmp.path()),
        second,
        tx2,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result2.session_id.as_deref(), Some(FRESH_ID));
    assert_eq!(
        state.spawns.load(Ordering::Relaxed),
        1,
        "second turn must NOT spawn a new process"
    );
    let events = drain_events(&mut rx2);
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentProviderEvent::Stream(StreamEvent::AssistantMessage { .. })
        )),
        "second turn must stream the new assistant reply (offset tracking): {events:?}"
    );
    let prompt_writes = {
        let writes = state.writes.lock().unwrap();
        writes.iter().filter(|w| w.contains("\u{1b}[200~")).count()
    };
    assert_eq!(prompt_writes, 2, "one prompt write per turn");
}
