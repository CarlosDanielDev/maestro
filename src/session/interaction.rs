//! Interactive (chat-style) session scaffold (#734).
//!
//! Type plumbing only — no spawn or UI behavior. The per-turn
//! `claude --resume` loop lands in #737; the Interaction screen in #736.
//! `InteractionState` models the long-lived conversation lifecycle and is
//! deliberately separate from `super::types::SessionStatus`, which models
//! one-shot work.

use super::interaction_lifecycle::InteractionLifecycleEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Lifecycle phase of an interactive session. `#737` drives
/// `Idle` ↔ `Streaming`; `Terminated` is set once on close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionState {
    #[default]
    Idle,
    Streaming,
    Terminated,
}

/// Author of a single conversation turn. Named `TurnRole` to avoid
/// collision with `super::role::Role` (agent personality), which is a
/// distinct concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Agent,
    System,
}

/// Why an interaction session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    PrCreated { pr_number: u64 },
    UserQuit,
    AgentFailure { tail: String },
}

/// One conversational turn. `finished_at` is `None` while a turn streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub role: TurnRole,
    pub content: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

/// Streaming events consumed by the Interaction screen (#738) as a turn
/// runs. Since #947 these are derived from the pipeline session's
/// `StreamEvent`s (`App::forward_interactive_stream_event`); before that
/// they were emitted by the retired `interaction_turn` bare-spawn loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    TurnStarted { role: TurnRole, at: DateTime<Utc> },
    Chunk(String),
    TurnFinished { at: DateTime<Utc> },
    Error(String),
}

/// Persisted state for one interactive session attached to an issue.
/// Scaffold only (#734): #736 renders it, #737 drives turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionSession {
    pub issue_number: u64,
    pub worktree_path: PathBuf,
    pub branch: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub state: InteractionState,
    #[serde(default)]
    pub history: Vec<TurnRecord>,
    pub produce_pr: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub close_reason: Option<CloseReason>,
    /// A terminator queued while a turn was streaming; fired once the turn
    /// returns to `Idle` (#739). `None` in the common Idle path (fired
    /// immediately). Purely in-memory turn-boundary state — never persisted.
    #[serde(skip)]
    pub queued_terminator: Option<InteractionLifecycleEvent>,
}

impl InteractionSession {
    /// Construct a fresh, open session. `created_at` is stamped now and
    /// `state` starts at `Idle`.
    #[allow(dead_code)] // Reason: scaffold for #736/#737 — constructed by the spawn loop
    pub fn new(
        issue_number: u64,
        worktree_path: PathBuf,
        branch: String,
        produce_pr: bool,
    ) -> Self {
        Self {
            issue_number,
            worktree_path,
            branch,
            session_id: None,
            state: InteractionState::Idle,
            history: Vec::new(),
            produce_pr,
            created_at: Utc::now(),
            closed_at: None,
            close_reason: None,
            queued_terminator: None,
        }
    }

    /// True while the session has not been terminated. Callers ask this
    /// instead of matching on `state` directly.
    pub fn is_active(&self) -> bool {
        self.state != InteractionState::Terminated
    }

    /// Signal that this session should terminate because a PR was linked
    /// (#739).
    ///
    /// - `Idle`: fire now — state → `Terminated`, stamp `close_reason`/
    ///   `closed_at`.
    /// - `Streaming`: queue it; the in-flight turn finishes untouched and the
    ///   turn-completion path fires it later (mid-turn deferral).
    /// - `Terminated`: idempotent no-op (debug-logged).
    pub fn signal_terminator(&mut self, event: InteractionLifecycleEvent) {
        match self.state {
            InteractionState::Terminated => {
                tracing::debug!(
                    issue_number = self.issue_number,
                    "terminator already fired; ignoring"
                );
            }
            InteractionState::Streaming => {
                self.queued_terminator = Some(event);
            }
            InteractionState::Idle => self.fire_terminator(event),
        }
    }

    /// Fire a terminator that was deferred while the session was `Streaming`
    /// (#936), now that the in-flight turn has settled back to `Idle` and its
    /// output has been merged. Called after the completing turn is written
    /// back into the pool.
    ///
    /// - `Idle` with a queued terminator → fire it (state → `Terminated`,
    ///   `close_reason` stamped, `closed_at` set, queue cleared).
    /// - `Terminated` → drop the queue. The session was already closed by other
    ///   means (e.g. the user quit mid-turn); never resurrect it (idempotent).
    /// - `Streaming`, or no queued terminator → no-op.
    pub fn settle_queued_terminator(&mut self) {
        match self.state {
            InteractionState::Idle => {
                if let Some(event) = self.queued_terminator.take() {
                    self.fire_terminator(event);
                }
            }
            InteractionState::Terminated => {
                self.queued_terminator = None;
            }
            InteractionState::Streaming => {}
        }
    }

    /// Apply a terminator immediately: `Terminated` + `close_reason` +
    /// `closed_at`. Only `PrLinkedToIssue` maps to a `CloseReason` today;
    /// other variants set the terminal state without a reason.
    fn fire_terminator(&mut self, event: InteractionLifecycleEvent) {
        if let InteractionLifecycleEvent::PrLinkedToIssue { pr_number, .. } = event {
            self.close_reason = Some(CloseReason::PrCreated { pr_number });
        }
        self.state = InteractionState::Terminated;
        self.closed_at = Some(Utc::now());
    }
}

#[cfg(test)]
#[path = "interaction_tests.rs"]
mod tests;
