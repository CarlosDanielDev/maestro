//! Per-turn `claude --resume` spawn loop for interactive sessions (#737).
//!
//! Each call to [`InteractionSession::send_turn`] spawns a fresh `claude`
//! process that resumes Claude CLI's persisted transcript. The spawn is hidden
//! behind the [`SpawnAgent`] trait so tests feed canned stream-json without a
//! real process; #751 will reimplement `SpawnAgent` on the `ClaudeTransport`
//! seam (from #748) without touching `send_turn`.
//!
//! Persistence is the caller's job: `send_turn` mutates `self` (history,
//! `session_id`, `state`) and returns; the owner of `MaestroState` persists via
//! `StateStore`. This keeps `send_turn` pure/unit-testable (RUST-GUARDRAILS §7)
//! and respects that the session does not own the state file.

use std::path::PathBuf;
use std::process::Stdio;

use chrono::Utc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::interaction::{InteractionSession, InteractionState, TurnRecord, TurnRole};
use super::parser::{extract_session_id, parse_stream_line};
use super::types::StreamEvent;

/// How many bytes of child stderr to retain for error reporting.
const STDERR_TAIL_CAP: usize = 2_000;
/// Bounded capacity for the per-turn stdout line channel.
const LINE_CHANNEL_CAP: usize = 64;

/// Args + working directory for one spawned turn. Built by
/// [`InteractionSession::build_turn_argv`]; consumed by [`SpawnAgent::spawn`].
/// The program name is owned by the spawner (e.g. [`ClaudeCliSpawner::binary`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnArgv {
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// Terminal summary of a spawned turn: process exit code + a capped stderr tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnOutcome {
    pub exit_code: Option<i32>,
    pub stderr_tail: String,
}

/// Live handle to a spawned turn: a bounded stream of stdout lines and a
/// one-shot that resolves once the process exits.
pub struct TurnHandle {
    pub lines: mpsc::Receiver<String>,
    pub outcome: oneshot::Receiver<TurnOutcome>,
}

/// Failure to launch the agent process.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("spawn failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("spawn failed: {0}")]
    Other(String),
}

/// Error returned by [`InteractionSession::send_turn`]. Cancellation is NOT an
/// error — it returns `Ok(())` after appending a `System` marker turn.
#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    /// The agent process failed to launch.
    #[error("failed to spawn claude: {0}")]
    Spawn(#[from] SpawnError),
    /// The agent process exited non-zero.
    #[error("agent exit {code:?}: {stderr_tail}")]
    NonZeroExit {
        code: Option<i32>,
        stderr_tail: String,
    },
}

/// Streaming events emitted to the caller (TUI screen, #738) as a turn runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    TurnStarted {
        role: TurnRole,
        at: chrono::DateTime<Utc>,
    },
    Chunk(String),
    TurnFinished {
        at: chrono::DateTime<Utc>,
    },
    Error(String),
}

/// Spawns one agent turn. Mockable so tests inject canned stream-json. The
/// production impl is [`ClaudeCliSpawner`]; #751 implements this on the
/// `ClaudeTransport` seam instead.
#[async_trait::async_trait]
pub trait SpawnAgent: Send + Sync {
    async fn spawn(
        &self,
        argv: TurnArgv,
        cancel: CancellationToken,
    ) -> Result<TurnHandle, SpawnError>;
}

/// Production [`SpawnAgent`]: launches the real `claude` CLI via
/// `tokio::process`. Mirrors the headless subprocess lifecycle
/// (RUST-GUARDRAILS §5): explicit pipes, background line readers,
/// `tokio::select!` cancel arm with `child.kill().await`.
pub struct ClaudeCliSpawner {
    binary: String,
}

impl ClaudeCliSpawner {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Default for ClaudeCliSpawner {
    fn default() -> Self {
        Self::new("claude")
    }
}

#[async_trait::async_trait]
impl SpawnAgent for ClaudeCliSpawner {
    async fn spawn(
        &self,
        argv: TurnArgv,
        cancel: CancellationToken,
    ) -> Result<TurnHandle, SpawnError> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(&argv.args)
            .current_dir(&argv.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(SpawnError::Io)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SpawnError::Other("no stdout from claude CLI".to_string()))?;
        let stderr = child.stderr.take();

        let (lines_tx, lines_rx) = mpsc::channel::<String>(LINE_CHANNEL_CAP);
        let (outcome_tx, outcome_rx) = oneshot::channel::<TurnOutcome>();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let stderr_handle = stderr.map(|s| {
                tokio::spawn(async move {
                    let mut buf = String::new();
                    let mut lines = BufReader::new(s).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if !buf.is_empty() {
                            buf.push('\n');
                        }
                        buf.push_str(&line);
                        if buf.len() > STDERR_TAIL_CAP {
                            let start = buf.len() - STDERR_TAIL_CAP;
                            buf = buf.split_off(start);
                        }
                    }
                    buf
                })
            });

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = child.kill().await;
                        break;
                    }
                    next = reader.next_line() => match next {
                        Ok(Some(line)) => {
                            if lines_tx.send(line).await.is_err() {
                                // Receiver dropped — stop reading, kill child.
                                let _ = child.kill().await;
                                break;
                            }
                        }
                        _ => break,
                    },
                }
            }

            let status = child.wait().await.ok();
            let stderr_tail = match stderr_handle {
                Some(h) => h.await.unwrap_or_default(),
                None => String::new(),
            };
            let _ = outcome_tx.send(TurnOutcome {
                exit_code: status.and_then(|s| s.code()),
                stderr_tail,
            });
        });

        Ok(TurnHandle {
            lines: lines_rx,
            outcome: outcome_rx,
        })
    }
}

impl InteractionSession {
    /// Build the argv for one turn. First turn (no `session_id`) starts a fresh
    /// transcript; subsequent turns resume via `--resume <id>`.
    pub fn build_turn_argv(&self, prompt: &str, model: &str) -> TurnArgv {
        let args = match self.session_id.as_deref() {
            None => vec![
                "--print".to_string(),
                "--verbose".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--model".to_string(),
                model.to_string(),
                "-p".to_string(),
                prompt.to_string(),
            ],
            Some(id) => vec![
                "--resume".to_string(),
                id.to_string(),
                "--print".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "-p".to_string(),
                prompt.to_string(),
            ],
        };
        TurnArgv {
            args,
            cwd: self.worktree_path.clone(),
        }
    }

    /// Run one conversational turn: spawn `claude`, stream chunks through
    /// `events_tx`, capture `session_id`, and append the turn to `history`.
    ///
    /// Mutates `self` (history, `session_id`, `state`) in place; the caller
    /// persists via `StateStore`. Cancellation is a clean `Ok(())` after a
    /// `System` marker turn. Stream parse errors are logged and skipped, never
    /// fatal.
    pub async fn send_turn(
        &mut self,
        prompt: String,
        model: &str,
        spawner: &dyn SpawnAgent,
        events_tx: mpsc::Sender<TurnEvent>,
        cancel: CancellationToken,
    ) -> Result<(), TurnError> {
        // argv is keyed on the *current* session_id, before this turn captures
        // a new one — build it first.
        let argv = self.build_turn_argv(&prompt, model);

        let user_at = Utc::now();
        self.history.push(TurnRecord {
            role: TurnRole::User,
            content: prompt,
            started_at: user_at,
            finished_at: Some(user_at),
        });
        self.state = InteractionState::Streaming;

        let mut handle = match spawner.spawn(argv, cancel.clone()).await {
            Ok(handle) => handle,
            Err(err) => {
                let _ = events_tx
                    .send(TurnEvent::Error(format!("failed to spawn claude: {err}")))
                    .await;
                self.state = InteractionState::Idle;
                return Err(TurnError::Spawn(err));
            }
        };

        let started_at = Utc::now();
        let _ = events_tx
            .send(TurnEvent::TurnStarted {
                role: TurnRole::Agent,
                at: started_at,
            })
            .await;
        let agent_idx = self.history.len();
        self.history.push(TurnRecord {
            role: TurnRole::Agent,
            content: String::new(),
            started_at,
            finished_at: None,
        });

        let mut cancelled = false;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    cancelled = true;
                    break;
                }
                line = handle.lines.recv() => match line {
                    Some(line) => self.apply_line(&line, agent_idx, &events_tx).await,
                    None => break,
                },
            }
        }

        let finished_at = Utc::now();
        self.history[agent_idx].finished_at = Some(finished_at);
        self.state = InteractionState::Idle;

        if cancelled {
            self.push_system("turn cancelled by user", finished_at);
            let _ = events_tx
                .send(TurnEvent::Error("turn cancelled by user".to_string()))
                .await;
            return Ok(());
        }

        if let Ok(outcome) = handle.outcome.await
            && let Some(code) = outcome.exit_code
            && code != 0
        {
            let msg = format!("agent exit {code}: {}", outcome.stderr_tail);
            let _ = events_tx.send(TurnEvent::Error(msg)).await;
            self.push_system(&format!("agent exit {code}"), finished_at);
            return Err(TurnError::NonZeroExit {
                code: Some(code),
                stderr_tail: outcome.stderr_tail,
            });
        }

        // Degraded mode: a successful turn that never yielded a session_id.
        // The next turn re-inits (build_turn_argv stays on the first-turn arm).
        if self.session_id.is_none() {
            self.push_system(
                "could not bind session_id; subsequent turns will re-init context",
                finished_at,
            );
        }

        let _ = events_tx
            .send(TurnEvent::TurnFinished { at: finished_at })
            .await;
        Ok(())
    }

    /// Apply one stdout line: capture `session_id` (first one wins), forward
    /// assistant text as a `Chunk`, log-and-skip unparseable lines.
    async fn apply_line(
        &mut self,
        line: &str,
        agent_idx: usize,
        events_tx: &mpsc::Sender<TurnEvent>,
    ) {
        if self.session_id.is_none()
            && let Some(id) = extract_session_id(line)
        {
            self.session_id = Some(id);
        }
        for event in parse_stream_line(line) {
            match event {
                StreamEvent::AssistantMessage { text } => {
                    self.history[agent_idx].content.push_str(&text);
                    let _ = events_tx.send(TurnEvent::Chunk(text)).await;
                }
                StreamEvent::Unknown { raw } => {
                    tracing::warn!(line = %raw, "interaction: unparsed stream-json line; skipping");
                }
                _ => {}
            }
        }
    }

    fn push_system(&mut self, content: &str, at: chrono::DateTime<Utc>) {
        self.history.push(TurnRecord {
            role: TurnRole::System,
            content: content.to_string(),
            started_at: at,
            finished_at: Some(at),
        });
    }
}
