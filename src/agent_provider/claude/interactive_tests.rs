//! Unit tests for the interactive PTY transport (issue #749), using a
//! trait-based mock backend per RUST-GUARDRAILS.md §7 — no real `claude`
//! binary, no real PTY.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::agent_provider::claude::pty_backend::PortablePtyBackend;
use crate::agent_provider::types::{AgentError, AgentProviderEvent, AgentRequest};
use crate::session::types::StreamEvent;

const MODE_LINE: &str = r#"{"type":"mode","mode":"normal","sessionId":"s"}"#;
const ASSISTANT_LINE: &str = r#"{"type":"assistant","message":{"id":"m1","type":"message","role":"assistant","model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"hi from mock"}],"stop_reason":"end_turn"}}"#;
const TURN_DURATION_LINE: &str =
    r#"{"type":"system","subtype":"turn_duration","durationMs":42,"messageCount":2}"#;

#[derive(Clone, Default)]
struct MockState {
    writes: Arc<Mutex<Vec<String>>>,
    killed: Arc<AtomicBool>,
}

struct MockChild {
    state: MockState,
    transcript: PathBuf,
    respond: bool,
    exited: Option<i32>,
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
        if text.starts_with("/exit") {
            self.exited = Some(0);
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

struct MockBackend {
    state: MockState,
    create_transcript: bool,
    respond: bool,
    spawn_delay: Option<Duration>,
    transcript: PathBuf,
}

impl MockBackend {
    fn new(transcript: PathBuf) -> Self {
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
    fn spawn(&self, _spec: &SpawnSpec) -> Result<Box<dyn InteractiveChild>, AgentError> {
        if let Some(delay) = self.spawn_delay {
            std::thread::sleep(delay);
        }
        if self.create_transcript {
            if let Some(parent) = self.transcript.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&self.transcript, format!("{MODE_LINE}\n")).unwrap();
        }
        Ok(Box::new(MockChild {
            state: self.state.clone(),
            transcript: self.transcript.clone(),
            respond: self.respond,
            exited: None,
        }))
    }
}

fn run_spec(home: &Path) -> RunSpec {
    RunSpec::new(
        "claude",
        "claude",
        home.to_path_buf(),
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    )
}

fn request(cwd: &Path) -> AgentRequest {
    let mut request = AgentRequest::stream_json("hello there".into(), "claude-haiku".into());
    request.cwd = Some(cwd.to_path_buf());
    request
}

fn drain_events(rx: &mut mpsc::UnboundedReceiver<AgentProviderEvent>) -> Vec<AgentProviderEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
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
async fn happy_path_streams_events_and_exits_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let spec = run_spec(tmp.path());
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), &spec.session_id);
    let backend = Arc::new(MockBackend::new(transcript));
    let state = backend.state.clone();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = run_interactive(backend, spec, request, tx, CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(result.exit_code, Some(0));

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

    let writes = state.writes.lock().unwrap();
    assert!(
        writes[0].starts_with("\u{1b}[200~") && writes[0].ends_with("\u{1b}[201~\r"),
        "prompt must be bracketed-paste wrapped: {writes:?}"
    );
    assert!(
        writes.iter().any(|w| w.starts_with("/exit")),
        "clean shutdown must send /exit: {writes:?}"
    );
    assert!(
        !state.killed.load(Ordering::Relaxed),
        "clean exit must not kill"
    );
}

#[tokio::test]
async fn cancel_sends_double_ctrl_c_then_kills() {
    let tmp = tempfile::tempdir().unwrap();
    let spec = run_spec(tmp.path());
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), &spec.session_id);
    let mut backend = MockBackend::new(transcript);
    backend.respond = false; // turn never completes
    let backend = Arc::new(backend);
    let state = backend.state.clone();

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        trigger.cancel();
    });

    let (tx, _rx) = mpsc::unbounded_channel();
    let started = std::time::Instant::now();
    let result = run_interactive(backend, spec, request, tx, cancel).await;

    assert!(
        matches!(result, Err(AgentError::Cancelled { .. })),
        "expected Cancelled, got: {result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "cancel must resolve quickly (got {:?})",
        started.elapsed()
    );
    let writes = state.writes.lock().unwrap();
    let ctrl_c = writes.iter().filter(|w| w.as_str() == "\u{3}").count();
    assert_eq!(ctrl_c, 2, "cancel must send Ctrl-C twice: {writes:?}");
    assert!(
        state.killed.load(Ordering::Relaxed),
        "child must be killed when Ctrl-C does not stop it"
    );
}

#[tokio::test]
async fn spawn_timeout_maps_to_spawn_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut spec = run_spec(tmp.path());
    spec.spawn_timeout = Duration::from_millis(100);
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), &spec.session_id);
    let mut backend = MockBackend::new(transcript);
    backend.spawn_delay = Some(Duration::from_secs(2));
    let backend = Arc::new(backend);

    let (tx, _rx) = mpsc::unbounded_channel();
    let result = run_interactive(backend, spec, request, tx, CancellationToken::new()).await;

    assert!(
        matches!(result, Err(AgentError::Spawn { .. })),
        "expected Spawn timeout error, got: {result:?}"
    );
}

/// End-to-end through the REAL `PortablePtyBackend` against a stub `claude`
/// shell script (testdata/claude-transcript/stub-claude.sh). Lives here
/// rather than in `src/integration_tests/` because that module's charter is
/// "no process spawning". Unix-only: the stub is a shell script.
#[cfg(unix)]
#[tokio::test]
async fn portable_pty_backend_runs_stub_claude_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    // macOS tempdirs live behind /var -> /private/var; the stub derives the
    // munged dir from its physical `pwd`, so canonicalize on our side too.
    let home = tmp.path().canonicalize().unwrap();

    let stub =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/claude-transcript/stub-claude.sh");
    assert!(stub.exists(), "stub script missing: {}", stub.display());

    let mut spec = RunSpec::new(
        stub.to_string_lossy().to_string(),
        "claude",
        home.clone(),
        "12345678-1234-1234-1234-123456789abc",
    );
    spec.extra_env = vec![("HOME".to_string(), home.to_string_lossy().to_string())];

    let mut request = AgentRequest::stream_json("ping".into(), "claude-haiku".into());
    request.cwd = Some(home.clone());

    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = run_interactive(
        Arc::new(PortablePtyBackend),
        spec,
        request,
        tx,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(
        result.exit_code,
        Some(0),
        "stub must exit cleanly via /exit"
    );

    let stream = drain_events(&mut rx)
        .into_iter()
        .filter_map(|e| match e {
            AgentProviderEvent::Stream(s) => Some(format!("{s:?}")),
            AgentProviderEvent::Started(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("interactive_stub_claude_stream", stream);
}

#[tokio::test]
async fn readiness_timeout_kills_child_and_reports_transcript_path() {
    let tmp = tempfile::tempdir().unwrap();
    let mut spec = run_spec(tmp.path());
    spec.readiness_timeout = Duration::from_millis(300);
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), &spec.session_id);
    let mut backend = MockBackend::new(transcript);
    backend.create_transcript = false; // boot never happens
    let backend = Arc::new(backend);
    let state = backend.state.clone();

    let (tx, _rx) = mpsc::unbounded_channel();
    let result = run_interactive(backend, spec, request, tx, CancellationToken::new()).await;

    match result {
        Err(AgentError::Stream(msg)) => {
            assert!(
                msg.contains("transcript never appeared"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected Stream error, got: {other:?}"),
    }
    assert!(
        state.killed.load(Ordering::Relaxed),
        "child must not be left running after readiness timeout"
    );
}
