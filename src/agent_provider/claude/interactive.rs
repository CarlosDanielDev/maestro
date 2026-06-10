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

pub(super) use super::interactive_types::{
    InteractiveBackend, InteractiveChild, RunSpec, SpawnSpec, build_args, scrub_env_names,
    transcript_path, turn_payload,
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Tail poll cadence for new transcript lines.
const TAIL_INTERVAL: Duration = Duration::from_millis(150);

/// A live PTY child bound to one Claude conversation, parked between turns
/// (#751). `offset` is how far into the transcript we have already parsed —
/// the next turn tails from there.
pub(super) struct ChildSlot {
    pub child: Box<dyn InteractiveChild>,
    pub transcript: PathBuf,
    pub offset: u64,
}

/// Children parked between turns, keyed by bound session id. Shared by all
/// clones of one `ClaudeProvider`; entries die with the provider (PTY
/// children are killed in their `Drop`). One concurrent turn per session id —
/// the TUI's `Streaming` state already serializes turns per interaction.
pub(super) type SessionSlots =
    Arc<tokio::sync::Mutex<std::collections::HashMap<String, ChildSlot>>>;

enum SpawnMode<'a> {
    /// New conversation: `--session-id <id>` pins the transcript path.
    Fresh(&'a str),
    /// Existing conversation (e.g. after a maestro restart): `--resume <id>`.
    Resume(&'a str),
}

/// Run one interaction turn over the interactive transport (#749, #751).
///
/// Conversation continuity rides on `request.resume_session_id`:
/// - `Some(id)` with a live parked child → reuse it (no new process);
/// - `Some(id)` without one → spawn `claude --resume <id>` and re-attach;
/// - `None` → fresh conversation: spawn `claude --session-id <new-uuid>`.
///
/// On a completed turn the child is parked for the next turn — `/exit` is
/// NOT sent. On cancellation the child gets Ctrl-C twice (100ms apart) and
/// stays parked if it survives; it is dropped (and killed) only if it died.
pub(super) async fn run_session_turn(
    backend: Arc<dyn InteractiveBackend>,
    slots: SessionSlots,
    spec: RunSpec,
    request: AgentRequest,
    events: mpsc::UnboundedSender<AgentProviderEvent>,
    cancel: CancellationToken,
) -> Result<AgentRunResult, AgentError> {
    // The resume id lands verbatim in argv (`--resume <id>`) and in the
    // transcript filename, and it can come from a persisted (tamperable)
    // state file — so it must pass the same allowlist the headless capture
    // path enforces. A rejected id falls back to a FRESH conversation
    // (degraded, like headless re-init) instead of reaching argv (#751 sec).
    let resume = request
        .resume_session_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .filter(|id| {
            let valid = crate::session::parser::is_valid_session_id(id);
            if !valid {
                warn!(
                    provider_id = %spec.provider_id,
                    "rejecting invalid resume_session_id; starting a fresh conversation"
                );
            }
            valid
        });

    let (bound_id, mut slot) = match resume {
        Some(id) => {
            let parked = slots.lock().await.remove(&id);
            let live = parked.and_then(|mut slot| {
                if matches!(slot.child.try_wait(), Ok(None)) {
                    Some(slot)
                } else {
                    None // dead child — drop (kill in Drop) and re-attach
                }
            });
            match live {
                Some(slot) => {
                    debug!(session_id = %id, "reusing parked interactive claude child");
                    (id, slot)
                }
                None => {
                    let slot =
                        spawn_slot(&backend, &spec, &request, SpawnMode::Resume(&id)).await?;
                    (id, slot)
                }
            }
        }
        None => {
            let id = spec.session_id.clone();
            let slot = spawn_slot(&backend, &spec, &request, SpawnMode::Fresh(&id)).await?;
            (id, slot)
        }
    };

    let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
        process_id: slot.child.process_id(),
    }));

    // On error the slot is dropped, so the next turn re-attaches.
    slot.child.write_text(&turn_payload(&request.prompt))?;

    // Blocking tail → bounded channel → caller's unbounded channel.
    let (tail_tx, mut tail_rx) = mpsc::channel::<StreamEvent>(64);
    let stop = Arc::new(AtomicBool::new(false));
    let consumed = Arc::new(std::sync::atomic::AtomicU64::new(slot.offset));
    let tail_stop = Arc::clone(&stop);
    let tail_consumed = Arc::clone(&consumed);
    let tail_path = slot.transcript.clone();
    let start_offset = slot.offset;
    let tail_task = tokio::task::spawn_blocking(move || {
        tail_transcript(
            &tail_path,
            start_offset,
            tail_tx,
            &tail_stop,
            &tail_consumed,
        )
    });

    let mut death_check = tokio::time::interval(Duration::from_millis(500));
    death_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    enum Outcome {
        Completed,
        Cancelled,
        ChildDied(Option<i32>),
    }

    let outcome = loop {
        tokio::select! {
            _ = cancel.cancelled() => break Outcome::Cancelled,
            _ = death_check.tick() => {
                if let Ok(Some(code)) = slot.child.try_wait() {
                    break Outcome::ChildDied(Some(code));
                }
            }
            event = tail_rx.recv() => {
                match event {
                    Some(StreamEvent::Completed { cost_usd }) => {
                        let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
                            cost_usd,
                        }));
                        break Outcome::Completed;
                    }
                    Some(event) => {
                        let _ = events.send(AgentProviderEvent::Stream(event));
                    }
                    None => break Outcome::ChildDied(None),
                }
            }
        }
    };

    stop.store(true, Ordering::Relaxed);
    drop(tail_rx);
    let _ = tail_task.await;
    slot.offset = consumed.load(Ordering::Relaxed);

    match outcome {
        Outcome::Completed => {
            // Park the child for the next turn — no /exit, no kill.
            slots.lock().await.insert(bound_id.clone(), slot);
            Ok(AgentRunResult {
                exit_code: None,
                session_id: Some(bound_id),
            })
        }
        Outcome::Cancelled => {
            // Interrupt the in-flight turn; keep the conversation alive.
            let _ = slot.child.write_text("\x03");
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = slot.child.write_text("\x03");
            if matches!(slot.child.try_wait(), Ok(None)) {
                slots.lock().await.insert(bound_id, slot);
            }
            // else: child died under Ctrl-C — drop the slot (kill in Drop).
            Err(AgentError::Cancelled {
                provider_id: spec.provider_id.clone(),
            })
        }
        Outcome::ChildDied(code) => {
            warn!(session_id = %bound_id, exit_code = ?code, "interactive claude child died mid-turn");
            Err(AgentError::FailedStatus {
                provider_id: spec.provider_id.clone(),
                status: code.map_or_else(|| "unknown".to_string(), |c| c.to_string()),
                stderr: "interactive claude child exited mid-turn (see transcript)".to_string(),
            })
        }
    }
}

/// Spawn a fresh or resumed PTY child, wait for its transcript, and return a
/// parked-ready slot. The starting offset skips already-recorded history on
/// resume so old turns are not replayed into the event stream.
async fn spawn_slot(
    backend: &Arc<dyn InteractiveBackend>,
    spec: &RunSpec,
    request: &AgentRequest,
    mode: SpawnMode<'_>,
) -> Result<ChildSlot, AgentError> {
    let cwd = match request.cwd.clone() {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|source| AgentError::Spawn {
            provider_id: spec.provider_id.clone(),
            source,
        })?,
    };
    let (id, id_args) = match mode {
        SpawnMode::Fresh(id) => (id, vec!["--session-id".to_string(), id.to_string()]),
        SpawnMode::Resume(id) => (id, vec!["--resume".to_string(), id.to_string()]),
    };
    let transcript = transcript_path(&spec.home_dir, &cwd, id);

    let env_remove = scrub_env_names(std::env::vars().map(|(k, _)| k));
    info!(
        removed_vars = ?env_remove,
        provider_id = %spec.provider_id,
        "scrubbing ANTHROPIC_* from interactive child env"
    );

    let mut args = id_args;
    args.extend(build_args(request));

    let spawn_spec = SpawnSpec {
        binary: spec.binary.clone(),
        args,
        cwd: Some(cwd),
        env_remove,
        env_set: spec.extra_env.clone(),
        cols: 132,
        rows: 40,
    };

    let spawn_backend = Arc::clone(backend);
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
                provider_id: spec.provider_id.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "interactive claude spawn timed out",
                ),
            });
        }
    };

    // Readiness: Claude Code writes the transcript's first line (`mode`) at
    // boot (fresh) or appends to the existing file (resume). Poll for file
    // existence instead of a flat sleep (spike #747).
    let ready_deadline = tokio::time::Instant::now() + spec.readiness_timeout;
    loop {
        if transcript.exists() {
            break;
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

    // Resume: skip recorded history; the TUI already holds it. Fresh: boot
    // metadata lines parse to nothing, so offset 0 is equivalent and simpler.
    let offset = match mode {
        SpawnMode::Fresh(_) => 0,
        SpawnMode::Resume(_) => std::fs::metadata(&transcript).map(|m| m.len()).unwrap_or(0),
    };

    Ok(ChildSlot {
        child,
        transcript,
        offset,
    })
}

/// Blocking transcript tail. Reads complete lines from `path` starting at
/// `start_offset`, parses each via [`parse_transcript_line`], pushes events
/// into the bounded channel, and publishes the consumed offset so the next
/// turn resumes where this one stopped. Exits when `stop` is set or the
/// receiver hangs up. Runs inside `spawn_blocking`.
fn tail_transcript(
    path: &Path,
    start_offset: u64,
    tx: mpsc::Sender<StreamEvent>,
    stop: &AtomicBool,
    consumed: &std::sync::atomic::AtomicU64,
) {
    let mut offset: u64 = start_offset;
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
                            consumed.store(offset, Ordering::Relaxed);
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

#[cfg(test)]
#[path = "interactive_session_tests.rs"]
mod session_tests;
