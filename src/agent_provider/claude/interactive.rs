//! Interactive (PTY) transport for the Claude CLI — issue #749.
//!
//! Drives `claude` as a long-lived interactive REPL inside a pseudo-terminal
//! so Claude Pro/Max subscription users keep subscription billing after the
//! 2026-06-15 headless cutoff. Mechanism per spike #747
//! (`docs/spikes/2026-05-claude-interactive-transport.md`):
//!
//! - the session id is generated up front and pinned with `--session-id`, so
//!   the transcript JSONL path is deterministic before spawn;
//! - machine-readable output comes from tailing that transcript (see
//!   [`super::transcript_parser`]), not from scraping the PTY screen — PTY
//!   output is drained and discarded;
//! - the child env is scrubbed of every `ANTHROPIC_*` variable so the child
//!   cannot silently fall back to API-key billing (security stake of
//!   milestone v0.30.5).
//!
//! Async discipline (RUST-GUARDRAILS.md §3): the blocking transcript tail
//! runs in `tokio::task::spawn_blocking` and feeds a **bounded**
//! `mpsc::channel(64)`; the async side forwards into the caller's event
//! channel and owns cancellation + child shutdown.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::agent_provider::types::{
    AgentError, AgentProviderEvent, AgentRequest, AgentRunResult, AgentRunStarted,
};
use crate::session::types::StreamEvent;

use super::transcript_parser::parse_transcript_line;

/// Env vars always removed from the interactive child (grep-able review
/// surface). The prefix sweep below catches future `ANTHROPIC_*` additions.
pub(super) const SCRUBBED_VARS: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];
/// Any var with this prefix is scrubbed, beyond the fixed list.
pub(super) const SCRUBBED_PREFIX: &str = "ANTHROPIC_";

const DEFAULT_SPAWN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
/// Grace window for the child to exit after `/exit` (success path).
const EXIT_GRACE: Duration = Duration::from_secs(3);
/// Grace window after the second Ctrl-C before a hard kill (cancel path).
const CANCEL_GRACE: Duration = Duration::from_secs(1);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Tail poll cadence for new transcript lines.
const TAIL_INTERVAL: Duration = Duration::from_millis(150);

/// What to spawn. Built by [`run_interactive`], consumed by a backend.
pub(super) struct SpawnSpec {
    pub binary: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Var *names* to remove from the child env.
    pub env_remove: Vec<String>,
    /// Extra vars to set (tests use this to redirect `HOME` at a stub).
    pub env_set: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

/// A spawned interactive child. Implementations must be cheap to call from
/// async context (writes are small and buffered).
pub(super) trait InteractiveChild: Send {
    fn write_text(&mut self, text: &str) -> Result<(), AgentError>;
    fn try_wait(&mut self) -> Result<Option<i32>, AgentError>;
    fn kill(&mut self) -> Result<(), AgentError>;
    fn process_id(&self) -> Option<u32>;
}

/// Spawns interactive children. `PortablePtyBackend` is the production impl;
/// tests inject mocks (RUST-GUARDRAILS.md §7).
pub(super) trait InteractiveBackend: Send + Sync {
    fn spawn(&self, spec: &SpawnSpec) -> Result<Box<dyn InteractiveChild>, AgentError>;
}

/// Per-run parameters that don't belong on [`AgentRequest`].
pub(super) struct RunSpec {
    pub binary: String,
    pub provider_id: String,
    /// Home directory of the *child* (transcript root). Production passes
    /// `$HOME`; tests pass a tempdir and mirror it via `extra_env`.
    pub home_dir: PathBuf,
    /// Pre-generated session UUID (pins the transcript path).
    pub session_id: String,
    pub extra_env: Vec<(String, String)>,
    pub spawn_timeout: Duration,
    pub readiness_timeout: Duration,
}

impl RunSpec {
    pub(super) fn new(
        binary: impl Into<String>,
        provider_id: impl Into<String>,
        home_dir: PathBuf,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            binary: binary.into(),
            provider_id: provider_id.into(),
            home_dir,
            session_id: session_id.into(),
            extra_env: Vec::new(),
            spawn_timeout: DEFAULT_SPAWN_TIMEOUT,
            readiness_timeout: DEFAULT_READINESS_TIMEOUT,
        }
    }
}

/// Claude Code's project-dir munge: `/` and `.` both become `-`.
pub(super) fn munge_project_dir(cwd: &Path) -> String {
    cwd.to_string_lossy().replace(['/', '.'], "-")
}

/// Transcript JSONL path for a session rooted at `home`.
pub(super) fn transcript_path(home: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    home.join(".claude")
        .join("projects")
        .join(munge_project_dir(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// Resolve the scrub list from an iterator of env var names: the fixed
/// [`SCRUBBED_VARS`] plus every name matching [`SCRUBBED_PREFIX`].
pub(super) fn scrub_env_names(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = SCRUBBED_VARS.iter().map(|s| s.to_string()).collect();
    for name in names {
        if name.starts_with(SCRUBBED_PREFIX) && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn build_args(request: &AgentRequest, session_id: &str) -> Vec<String> {
    let mut args = vec![
        "--session-id".to_string(),
        session_id.to_string(),
        "--model".to_string(),
        request.model.clone(),
    ];
    if let Some(mode) = request.permission_mode.as_deref()
        && !mode.is_empty()
        && mode != "default"
    {
        args.push("--permission-mode".to_string());
        args.push(mode.to_string());
    }
    args
}

/// Wrap the prompt in bracketed-paste markers so embedded newlines do not
/// submit early, then submit with `\r` (spike #747: `\r` submits, `\n`
/// inserts a newline in the composer).
fn turn_payload(prompt: &str) -> String {
    format!("\x1b[200~{prompt}\x1b[201~\r")
}

/// Run one interaction turn over the interactive transport.
///
/// Spawns the child, waits for the transcript file to appear (boot marker),
/// writes the prompt, tails the transcript into `events`, and finishes when
/// the `turn_duration` marker maps to [`StreamEvent::Completed`]. On success
/// the child is asked to `/exit` and reaped; on cancellation it gets Ctrl-C
/// twice (100ms apart) and a hard kill after 1s.
pub(super) async fn run_interactive(
    backend: Arc<dyn InteractiveBackend>,
    spec: RunSpec,
    request: AgentRequest,
    events: mpsc::UnboundedSender<AgentProviderEvent>,
    cancel: CancellationToken,
) -> Result<AgentRunResult, AgentError> {
    let cwd = match request.cwd.clone() {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|source| AgentError::Spawn {
            provider_id: spec.provider_id.clone(),
            source,
        })?,
    };
    let transcript = transcript_path(&spec.home_dir, &cwd, &spec.session_id);

    let env_remove = scrub_env_names(std::env::vars().map(|(k, _)| k));
    info!(
        removed_vars = ?env_remove,
        provider_id = %spec.provider_id,
        "scrubbing ANTHROPIC_* from interactive child env"
    );

    let spawn_spec = SpawnSpec {
        binary: spec.binary.clone(),
        args: build_args(&request, &spec.session_id),
        cwd: Some(cwd),
        env_remove,
        env_set: spec.extra_env.clone(),
        cols: 132,
        rows: 40,
    };

    let spawn_backend = Arc::clone(&backend);
    let provider_id = spec.provider_id.clone();
    let spawned = tokio::time::timeout(
        spec.spawn_timeout,
        tokio::task::spawn_blocking(move || spawn_backend.spawn(&spawn_spec)),
    )
    .await;
    let mut child = match spawned {
        Ok(Ok(result)) => result?,
        Ok(Err(join_err)) => {
            return Err(AgentError::Stream(format!(
                "interactive spawn task failed: {join_err}"
            )));
        }
        Err(_elapsed) => {
            return Err(AgentError::Spawn {
                provider_id,
                source: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "interactive claude spawn timed out",
                ),
            });
        }
    };

    let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
        process_id: child.process_id(),
    }));

    // Readiness: Claude Code writes the transcript's first line (`mode`) at
    // boot. Poll for file existence instead of a flat sleep (spike #747).
    let ready_deadline = tokio::time::Instant::now() + spec.readiness_timeout;
    loop {
        if transcript.exists() {
            break;
        }
        if cancel.is_cancelled() {
            return cancel_child(child.as_mut(), &spec.provider_id).await;
        }
        if tokio::time::Instant::now() >= ready_deadline {
            let _ = child.kill();
            return Err(AgentError::Stream(format!(
                "interactive claude transcript never appeared at {}",
                transcript.display()
            )));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    debug!(transcript = %transcript.display(), "interactive transcript ready");

    child.write_text(&turn_payload(&request.prompt))?;

    // Blocking tail → bounded channel → caller's unbounded channel.
    let (tail_tx, mut tail_rx) = mpsc::channel::<StreamEvent>(64);
    let stop = Arc::new(AtomicBool::new(false));
    let tail_stop = Arc::clone(&stop);
    let tail_path = transcript.clone();
    let tail_task =
        tokio::task::spawn_blocking(move || tail_transcript(&tail_path, tail_tx, &tail_stop));

    let outcome = loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                break None;
            }
            event = tail_rx.recv() => {
                match event {
                    Some(StreamEvent::Completed { cost_usd }) => {
                        let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
                            cost_usd,
                        }));
                        break Some(());
                    }
                    Some(event) => {
                        let _ = events.send(AgentProviderEvent::Stream(event));
                    }
                    None => {
                        break None;
                    }
                }
            }
        }
    };

    stop.store(true, Ordering::Relaxed);
    drop(tail_rx);
    let _ = tail_task.await;

    match outcome {
        Some(()) => {
            // Clean shutdown: ask the REPL to exit, then reap.
            let _ = child.write_text("/exit\r");
            let exit_code = wait_for_exit(child.as_mut(), EXIT_GRACE).await;
            if exit_code.is_none() {
                warn!("interactive claude did not exit after /exit; killing");
                let _ = child.kill();
            }
            Ok(AgentRunResult { exit_code })
        }
        None => cancel_child(child.as_mut(), &spec.provider_id).await,
    }
}

/// Cancel path: Ctrl-C twice with a 100ms gap, then hard kill after 1s.
async fn cancel_child(
    child: &mut dyn InteractiveChild,
    provider_id: &str,
) -> Result<AgentRunResult, AgentError> {
    let _ = child.write_text("\x03");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = child.write_text("\x03");
    if wait_for_exit(child, CANCEL_GRACE).await.is_none() {
        let _ = child.kill();
    }
    Err(AgentError::Cancelled {
        provider_id: provider_id.to_string(),
    })
}

async fn wait_for_exit(child: &mut dyn InteractiveChild, grace: Duration) -> Option<i32> {
    let deadline = tokio::time::Instant::now() + grace;
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(code)) = child.try_wait() {
            return Some(code);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    None
}

/// Blocking transcript tail. Reads complete lines from `path` starting at
/// offset 0, parses each via [`parse_transcript_line`], and pushes events
/// into the bounded channel. Exits when `stop` is set or the receiver hangs
/// up. Runs inside `spawn_blocking`.
fn tail_transcript(path: &Path, tx: mpsc::Sender<StreamEvent>, stop: &AtomicBool) {
    let mut offset: u64 = 0;
    let mut pending = String::new();
    while !stop.load(Ordering::Relaxed) {
        if let Ok(file) = std::fs::File::open(path) {
            let mut reader = BufReader::new(file);
            if reader.seek(SeekFrom::Start(offset)).is_ok() {
                loop {
                    pending.clear();
                    match reader.read_line(&mut pending) {
                        Ok(0) => break,
                        Ok(n) => {
                            // Only consume complete lines; a partial tail is
                            // re-read on the next pass.
                            if !pending.ends_with('\n') {
                                break;
                            }
                            offset += n as u64;
                            for event in parse_transcript_line(&pending) {
                                if tx.blocking_send(event).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        std::thread::sleep(TAIL_INTERVAL);
    }
}

#[cfg(test)]
#[path = "interactive_tests.rs"]
mod tests;
