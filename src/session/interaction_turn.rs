//! Per-turn agent loop for interactive sessions (#737, rewired in #751).
//!
//! Each call to [`InteractionSession::send_turn`] runs one conversational
//! turn through [`AgentProvider::run`] — the session no longer spawns
//! `claude` itself. Whichever transport the user configured does the work:
//! under `transport = "headless"` the provider spawns a fresh
//! `claude --resume <id>` per turn (issue #737's semantics, now expressed as
//! `AgentRequest::resume_session_id`); under `transport = "interactive"` the
//! provider writes the turn into its long-lived PTY child and no new process
//! is spawned. `send_turn` cannot tell the difference — events and the
//! returned session id flow through the same seam.
//!
//! Persistence is the caller's job: `send_turn` mutates `self` (history,
//! `session_id`, `state`) and returns; the owner of `MaestroState` persists
//! via `StateStore`. This keeps `send_turn` unit-testable with a mock
//! provider (RUST-GUARDRAILS §7) and respects that the session does not own
//! the state file.

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::types::{AgentError, AgentProvider, AgentProviderEvent, AgentRequest};

use super::interaction::{InteractionSession, InteractionState, TurnRecord, TurnRole};
use super::types::StreamEvent;

/// Error returned by [`InteractionSession::send_turn`]. Cancellation is NOT an
/// error — it returns `Ok(())` after appending a `System` marker turn.
#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    /// The agent run could not start (spawn/config failure).
    #[error("failed to start agent turn: {0}")]
    Spawn(String),
    /// The agent process exited non-zero (or the provider reported failure).
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

impl InteractionSession {
    /// Build the provider request for one turn. The first turn has no
    /// `resume_session_id` and starts a fresh conversation; subsequent turns
    /// resume the bound id (headless: `--resume <id>`; interactive: the
    /// provider's parked PTY child for that id).
    pub fn build_turn_request(&self, prompt: &str, model: &str) -> AgentRequest {
        let mut request = AgentRequest::stream_json(prompt.to_string(), model.to_string());
        request.cwd = Some(self.worktree_path.clone());
        request.resume_session_id = self.session_id.clone();
        request
    }

    /// Run one conversational turn through the configured provider: stream
    /// chunks through `events_tx`, bind `session_id` from the run result,
    /// and append the turn to `history`.
    ///
    /// Mutates `self` (history, `session_id`, `state`) in place; the caller
    /// persists via `StateStore`. Cancellation is a clean `Ok(())` after a
    /// `System` marker turn. Stream parse errors are logged and skipped,
    /// never fatal.
    pub async fn send_turn(
        &mut self,
        prompt: String,
        model: &str,
        provider: Arc<dyn AgentProvider>,
        events_tx: mpsc::Sender<TurnEvent>,
        cancel: CancellationToken,
    ) -> Result<(), TurnError> {
        // Lifecycle span (#742). `duration_ms`/`chunk_count` are recorded once
        // the turn settles. Held only across the sync prologue; the async body
        // re-enters it per await via `tracing::Instrument` semantics being
        // unnecessary here because every record happens at the end, in scope.
        let turn_index = self
            .history
            .iter()
            .filter(|t| t.role == TurnRole::User)
            .count()
            + 1;
        let span = tracing::info_span!(
            "interaction.turn",
            issue = self.issue_number,
            turn_index,
            duration_ms = tracing::field::Empty,
            chunk_count = tracing::field::Empty,
        );

        // The request is keyed on the *current* session_id, before this turn
        // binds a new one — build it first.
        let request = self.build_turn_request(&prompt, model);

        let user_at = Utc::now();
        self.history.push(TurnRecord {
            role: TurnRole::User,
            content: prompt,
            started_at: user_at,
            finished_at: Some(user_at),
        });
        self.state = InteractionState::Streaming;

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

        let (provider_tx, mut provider_rx) = mpsc::unbounded_channel::<AgentProviderEvent>();
        let run_cancel = cancel.clone();
        let run_task =
            tokio::spawn(async move { provider.run(request, provider_tx, run_cancel).await });

        // Drain provider events until the channel closes (the provider drops
        // its sender when the run returns). Cancellation flows through the
        // shared token — the provider kills/interrupts its child and the
        // drain loop ends naturally.
        let mut cancelled = false;
        let mut chunk_count: usize = 0;
        loop {
            tokio::select! {
                _ = cancel.cancelled(), if !cancelled => {
                    cancelled = true;
                    // keep draining: the provider still flushes its tail
                }
                event = provider_rx.recv() => match event {
                    Some(AgentProviderEvent::Stream(stream_event)) => {
                        if matches!(stream_event, StreamEvent::AssistantMessage { .. }) {
                            chunk_count += 1;
                        }
                        self.apply_stream_event(stream_event, agent_idx, &events_tx).await;
                    }
                    Some(AgentProviderEvent::Started(_)) => {}
                    None => break,
                },
            }
        }

        let run_result = run_task.await;
        let finished_at = Utc::now();
        self.history[agent_idx].finished_at = Some(finished_at);
        self.state = InteractionState::Idle;
        span.record(
            "duration_ms",
            (finished_at - started_at).num_milliseconds().max(0),
        );
        span.record("chunk_count", chunk_count as u64);
        let _guard = span.enter();

        let outcome = match run_result {
            Ok(outcome) => outcome,
            Err(join_err) => {
                let msg = format!("agent turn task failed: {join_err}");
                let _ = events_tx.send(TurnEvent::Error(msg.clone())).await;
                self.push_system(&msg, finished_at);
                return Err(TurnError::Spawn(msg));
            }
        };

        match outcome {
            Ok(result) => {
                // First bound id wins; later turns resume it.
                if self.session_id.is_none() {
                    self.session_id = result.session_id;
                }
                if let Some(code) = result.exit_code
                    && code != 0
                {
                    let msg = format!("agent exit {code}");
                    let _ = events_tx.send(TurnEvent::Error(msg.clone())).await;
                    self.push_system(&msg, finished_at);
                    return Err(TurnError::NonZeroExit {
                        code: Some(code),
                        stderr_tail: String::new(),
                    });
                }
            }
            Err(AgentError::Cancelled { .. }) => {
                cancelled = true;
            }
            Err(err @ (AgentError::Spawn { .. } | AgentError::Config(_))) => {
                let msg = format!("failed to spawn agent: {err}");
                let _ = events_tx.send(TurnEvent::Error(msg.clone())).await;
                self.push_system(&msg, finished_at);
                return Err(TurnError::Spawn(err.to_string()));
            }
            Err(AgentError::FailedStatus { status, stderr, .. }) => {
                let msg = format!("agent exit {status}: {stderr}");
                let _ = events_tx.send(TurnEvent::Error(msg)).await;
                self.push_system(&format!("agent exit {status}"), finished_at);
                return Err(TurnError::NonZeroExit {
                    code: status.parse::<i32>().ok(),
                    stderr_tail: stderr,
                });
            }
            Err(AgentError::Stream(msg)) => {
                let _ = events_tx.send(TurnEvent::Error(msg.clone())).await;
                self.push_system(&msg, finished_at);
                return Err(TurnError::NonZeroExit {
                    code: None,
                    stderr_tail: msg,
                });
            }
        }

        if cancelled {
            self.push_system("turn cancelled by user", finished_at);
            let _ = events_tx
                .send(TurnEvent::Error("turn cancelled by user".to_string()))
                .await;
            return Ok(());
        }

        // Degraded mode: a successful turn that never yielded a session_id.
        // The next turn re-inits (build_turn_request sends no resume id).
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

    /// Apply one provider stream event: forward assistant text as a `Chunk`,
    /// log-and-skip unparseable lines, ignore everything else (cost/context
    /// updates belong to the one-shot session pipeline).
    async fn apply_stream_event(
        &mut self,
        event: StreamEvent,
        agent_idx: usize,
        events_tx: &mpsc::Sender<TurnEvent>,
    ) {
        match event {
            StreamEvent::AssistantMessage { text } => {
                self.history[agent_idx].content.push_str(&text);
                let _ = events_tx.send(TurnEvent::Chunk(text)).await;
            }
            StreamEvent::Unknown { raw } => {
                tracing::warn!(line = %raw, "interaction: unparsed stream line; skipping");
            }
            _ => {}
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
