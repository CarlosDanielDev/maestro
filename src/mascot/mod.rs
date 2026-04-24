pub mod animator;
pub mod frames;
pub mod state;
pub mod widget;

#[cfg(test)]
mod tests;

pub use animator::MascotAnimator;
pub use state::MascotState;

use crate::session::types::SessionStatus;
use serde::{Deserialize, Serialize};

/// Rendering style for the mascot widget. Selects between the legacy Unicode
/// block-character art and the 1-bit pixel-art sprites rendered via half-block
/// encoding. Serde variants are lowercase (`"sprite"` / `"ascii"`) for
/// human-friendly TOML.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MascotStyle {
    #[default]
    Sprite,
    Ascii,
}

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
