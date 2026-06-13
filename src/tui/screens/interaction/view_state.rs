//! View projection + close-reason for the Interaction screen.
//!
//! Since #950 (screen-as-view) the screen owns no transcript: it renders
//! [`InteractionView`], a per-frame projection of the live
//! [`crate::session::types::Session`] (its turns, `turn_state`,
//! `settled_from`, and `pr_linked`) pushed in by the app, mirroring the
//! existing `set_spinner_context` injection. The Idle/Streaming lifecycle now
//! reads straight from `Session::turn_state`; [`CloseReason`] survives as the
//! quit-teardown banner reason (#949), which is screen-local and not part of
//! the session state.

use crate::session::interaction::TurnRecord;
use crate::session::interaction::TurnState;
use crate::session::types::{Session, SessionStatus};
use serde::{Deserialize, Serialize};

/// Read-only projection of the live `Session` the screen renders this frame.
/// Built by the app from the pool's `Session`; the screen owns no turns.
/// Defaults to an empty Idle view so a freshly-constructed screen draws the
/// starter hint before the first `set_view`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InteractionView {
    /// The transcript — projected from `Session::turns`.
    pub turns: Vec<TurnRecord>,
    /// Idle/Streaming — projected from `Session::turn_state`. Drives the lock.
    pub turn_state: TurnState,
    /// How the underlying one-shot run settled — projected from
    /// `Session::settled_from`. Drives the status banner (#950).
    pub settled_from: Option<SessionStatus>,
    /// Linked `/pushup` PR — projected from `Session::pr_linked`. Shown in
    /// the status banner.
    pub pr_linked: Option<u64>,
}

impl InteractionView {
    /// Project a live session into the render view (#950).
    pub(crate) fn from_session(session: &Session) -> Self {
        Self {
            turns: session.turns.clone(),
            turn_state: session.turn_state,
            settled_from: session.settled_from,
            pr_linked: session.pr_linked,
        }
    }

    /// Status-banner text derived from `settled_from` + `pr_linked` (#950),
    /// or `None` until the session settles.
    pub(crate) fn banner(&self) -> Option<String> {
        crate::session::interaction::settled_banner(self.settled_from, self.pr_linked)
    }
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
