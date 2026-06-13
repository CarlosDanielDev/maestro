//! Turn vocabulary for interactive sessions.
//!
//! #948 retired the parallel `InteractionSession`/`InteractionState`
//! machine: the session lifecycle lives on the unified
//! [`super::types::Session`] (`SessionStatus::Interactive` +
//! `Session::settled_from`), the transcript on `Session::turns`, and the
//! deferred terminator on `ManagedSession::queued_terminator`. What
//! remains here is the turn vocabulary shared by the engine and the
//! Interaction screen.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Turn-level activity inside a kept-alive Interactive session (#948).
/// Drives the input lock while a follow-up streams. In-memory only —
/// distinct from `SessionStatus`, which models the session lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnState {
    #[default]
    Idle,
    Streaming,
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
