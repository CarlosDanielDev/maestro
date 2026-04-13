pub mod animator;
pub mod eyes;
pub mod frames;
pub mod state;
pub mod widget;

#[cfg(test)]
mod tests;

pub use animator::MascotAnimator;
pub use state::MascotState;

use crate::session::types::{Session, SessionStatus};
use std::time::Duration;

/// Derives the mascot display state from session statuses.
/// Priority: Error > Conducting > Happy > Idle
pub fn derive_dashboard_mascot_state<'a>(
    statuses: impl Iterator<Item = &'a SessionStatus>,
) -> MascotState {
    let mut has_running = false;
    let mut has_errored = false;
    let mut all_completed = true;
    let mut any = false;

    for status in statuses {
        any = true;
        match status {
            SessionStatus::Errored => has_errored = true,
            SessionStatus::Running | SessionStatus::Spawning => has_running = true,
            SessionStatus::Completed => {}
            _ => all_completed = false,
        }
        if !matches!(status, SessionStatus::Completed) {
            all_completed = false;
        }
    }

    if !any {
        return MascotState::Idle;
    }

    if has_errored {
        MascotState::Error
    } else if has_running {
        MascotState::Conducting
    } else if all_completed {
        MascotState::Happy
    } else {
        MascotState::Idle
    }
}

/// Silence threshold for per-session thinking detection.
#[allow(dead_code)]
const SILENCE_THRESHOLD: Duration = Duration::from_secs(3);

/// Derives the mascot state for a single session's detail view.
#[allow(dead_code)]
pub fn derive_session_mascot_state(session: &Session) -> MascotState {
    match session.status {
        SessionStatus::Errored => MascotState::Error,
        SessionStatus::Completed => MascotState::Happy,
        SessionStatus::Paused => MascotState::Sleeping,
        SessionStatus::Running | SessionStatus::Spawning => {
            if session.is_thinking {
                MascotState::Thinking
            } else if let Some(started) = session.thinking_started_at {
                if started.elapsed() >= SILENCE_THRESHOLD {
                    MascotState::Thinking
                } else {
                    MascotState::Conducting
                }
            } else {
                MascotState::Conducting
            }
        }
        _ => MascotState::Idle,
    }
}
