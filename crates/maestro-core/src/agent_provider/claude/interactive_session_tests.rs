//! Resume / cancel / timeout / security / e2e tests for the interactive
//! transport — split from `interactive_tests.rs` (400-line guardrail).
//! Shares the mock backend defined there.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::tests::{
    ASSISTANT_LINE, FRESH_ID, MODE_LINE, MockBackend, TURN_DURATION_LINE, drain_events,
    empty_slots, request, run_spec,
};
use super::*;
use crate::agent_provider::claude::pty_backend::PortablePtyBackend;
use crate::agent_provider::types::{AgentError, AgentProviderEvent, AgentRequest};
use crate::session::types::StreamEvent;

#[tokio::test]
async fn resume_without_parked_child_spawns_with_resume_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let transcript = transcript_path(
        tmp.path(),
        tmp.path(),
        "11111111-2222-3333-4444-555555555555",
    );
    // Simulate prior history: transcript already exists with old content the
    // turn must NOT replay.
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(
        &transcript,
        format!("{MODE_LINE}\n{ASSISTANT_LINE}\n{TURN_DURATION_LINE}\n"),
    )
    .unwrap();

    let backend = Arc::new(MockBackend::new(transcript));
    let state = backend.state.clone();
    let slots = empty_slots();

    let mut req = request(tmp.path());
    req.resume_session_id = Some("11111111-2222-3333-4444-555555555555".to_string());

    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = run_session_turn(
        backend,
        Arc::clone(&slots),
        run_spec(tmp.path()),
        req,
        tx,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(
        result.session_id.as_deref(),
        Some("11111111-2222-3333-4444-555555555555")
    );
    let spawn_args: Vec<Vec<String>> = state.spawn_args.lock().unwrap().clone();
    assert!(
        spawn_args[0]
            .windows(2)
            .any(|w| w == ["--resume", "11111111-2222-3333-4444-555555555555"]),
        "re-attach spawn must use --resume: {spawn_args:?}"
    );
    // History skip: exactly ONE assistant message (the new one), not two.
    let assistant_count = drain_events(&mut rx)
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentProviderEvent::Stream(StreamEvent::AssistantMessage { .. })
            )
        })
        .count();
    assert_eq!(
        assistant_count, 1,
        "resume must tail from the end of the existing transcript"
    );
}

#[tokio::test]
async fn cancel_sends_double_ctrl_c_and_keeps_live_child_parked() {
    let tmp = tempfile::tempdir().unwrap();
    let spec = run_spec(tmp.path());
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), FRESH_ID);
    let mut backend = MockBackend::new(transcript);
    backend.respond = false; // turn never completes
    let backend = Arc::new(backend);
    let state = backend.state.clone();
    let slots = empty_slots();

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        trigger.cancel();
    });

    let (tx, _rx) = mpsc::unbounded_channel();
    let started = std::time::Instant::now();
    let result = run_session_turn(backend, Arc::clone(&slots), spec, request, tx, cancel).await;

    assert!(
        matches!(result, Err(AgentError::Cancelled { .. })),
        "expected Cancelled, got: {result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "cancel must resolve quickly (got {:?})",
        started.elapsed()
    );
    let ctrl_c = {
        let writes = state.writes.lock().unwrap();
        writes.iter().filter(|w| w.as_str() == "\u{3}").count()
    };
    assert_eq!(ctrl_c, 2, "cancel must send Ctrl-C twice");
    assert!(
        !state.killed.load(Ordering::Relaxed),
        "a live child survives cancellation (conversation stays warm)"
    );
    assert!(
        slots.lock().await.contains_key(FRESH_ID),
        "the surviving child must stay parked"
    );
}

#[tokio::test]
async fn spawn_timeout_maps_to_spawn_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut spec = run_spec(tmp.path());
    spec.spawn_timeout = Duration::from_millis(100);
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), FRESH_ID);
    let mut backend = MockBackend::new(transcript);
    backend.spawn_delay = Some(Duration::from_secs(2));
    let backend = Arc::new(backend);

    let (tx, _rx) = mpsc::unbounded_channel();
    let result = run_session_turn(
        backend,
        empty_slots(),
        spec,
        request,
        tx,
        CancellationToken::new(),
    )
    .await;

    assert!(
        matches!(result, Err(AgentError::Spawn { .. })),
        "expected Spawn timeout error, got: {result:?}"
    );
}

#[tokio::test]
async fn readiness_timeout_kills_child_and_reports_transcript_path() {
    let tmp = tempfile::tempdir().unwrap();
    let mut spec = run_spec(tmp.path());
    spec.readiness_timeout = Duration::from_millis(300);
    let request = request(tmp.path());
    let transcript = transcript_path(tmp.path(), tmp.path(), FRESH_ID);
    let mut backend = MockBackend::new(transcript);
    backend.create_transcript = false; // boot never happens
    let backend = Arc::new(backend);
    let state = backend.state.clone();

    let (tx, _rx) = mpsc::unbounded_channel();
    let result = run_session_turn(
        backend,
        empty_slots(),
        spec,
        request,
        tx,
        CancellationToken::new(),
    )
    .await;

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

    let slots = empty_slots();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = run_session_turn(
        Arc::new(PortablePtyBackend),
        Arc::clone(&slots),
        spec,
        request,
        tx,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(
        result.session_id.as_deref(),
        Some("12345678-1234-1234-1234-123456789abc")
    );
    assert_eq!(result.exit_code, None, "child is parked, not exited");
    assert_eq!(slots.lock().await.len(), 1, "stub child must be parked");

    let stream = drain_events(&mut rx)
        .into_iter()
        .filter_map(|e| match e {
            AgentProviderEvent::Stream(s) => Some(format!("{s:?}")),
            AgentProviderEvent::Started(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("interactive_stub_claude_stream", stream);

    // Dropping the slots kills the parked stub child (PtyChild::Drop).
    slots.lock().await.clear();
}

#[tokio::test]
async fn malicious_resume_id_never_reaches_argv_or_path() {
    // #751 security review (HIGH): a tampered state file must not be able to
    // inject argv options or traverse the transcript path. A flag-shaped or
    // path-shaped resume id falls back to a FRESH --session-id spawn.
    for evil in ["--dangerously-skip-permissions", "../../etc/passwd", "a b"] {
        let tmp = tempfile::tempdir().unwrap();
        let transcript = transcript_path(tmp.path(), tmp.path(), FRESH_ID);
        let backend = Arc::new(MockBackend::new(transcript));
        let state = backend.state.clone();

        let mut req = request(tmp.path());
        req.resume_session_id = Some(evil.to_string());

        let (tx, _rx) = mpsc::unbounded_channel();
        let result = run_session_turn(
            backend,
            empty_slots(),
            run_spec(tmp.path()),
            req,
            tx,
            CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(
            result.session_id.as_deref(),
            Some(FRESH_ID),
            "rejected id must bind a fresh conversation, not {evil:?}"
        );
        let spawn_args: Vec<Vec<String>> = state.spawn_args.lock().unwrap().clone();
        assert!(
            !spawn_args[0].iter().any(|a| a == evil),
            "evil id must never reach argv: {spawn_args:?}"
        );
        assert!(
            spawn_args[0].iter().any(|a| a == "--session-id"),
            "fresh spawn expected: {spawn_args:?}"
        );
    }
}
