//! Library facade — exposes only self-contained modules for benchmarks.
//!
//! Crate-wide lint policy lives in `Cargo.toml` under `[lints]`; see
//! `docs/RUST-GUARDRAILS.md` for the full policy document.

#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::significant_drop_tightening)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::branches_sharing_code)]

pub mod icon_mode;
pub mod icons;
pub mod turboquant;

#[path = "util"]
pub mod util {
    pub mod formatting;
    pub use formatting::*;
}

#[path = "session"]
pub mod session {
    pub mod intent;
    pub mod parser;
    pub mod role;
    pub mod transition;
    pub mod types;
}

#[path = "settings"]
pub mod settings {
    pub mod claude_settings;
    pub use claude_settings::{
        CavemanModeState, CavemanWriteError, FsSettingsStore, SettingsStore,
    };
}

// Spike prototype for ADR 002 (agent personalities). Gated behind --features
// spike so cargo build never includes it. Files live under
// src/tui/agent_personalities/ but are exposed at the lib facade root so the
// throwaway examples/agent_personalities_spike.rs binary can reach them.
// Removed when the ADR-002 cleanup commit lands.
#[cfg(feature = "spike")]
#[path = "tui/agent_personalities"]
pub mod agent_personalities {
    pub mod palette;
    pub mod render;
    pub mod role;
    pub mod sprite;
}
