//! View-local lifecycle state for the Interaction screen.
//!
//! These were the domain enums on `InteractionSession` until #948 retired
//! that struct: the session lifecycle now lives on the unified
//! [`crate::session::types::Session`] (`SessionStatus::Interactive` +
//! `turn_state`), and these enums survive ONLY as the screen's render
//! state. #950 (screen-as-view) derives the view directly from the live
//! `Session` and deletes this module.

use serde::{Deserialize, Serialize};

/// Lifecycle phase of the interaction view. `Idle` ↔ `Streaming` drive the
/// input lock; `Terminated` renders the closing banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionState {
    #[default]
    Idle,
    Streaming,
    Terminated,
}

/// Why the interaction view ended (banner text). Since #949 a PR no
/// longer closes the session — only quit (`UserQuit`) and a failed quit
/// teardown (`AgentFailure`) remain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    UserQuit,
    AgentFailure { tail: String },
}
