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

// Spike-only module (issue #513, ADR 001). Never landed on `main`.
// Cleanup: `git rm -r src/agent_graph_spike examples/agent_graph_spike.rs` and
// remove the `[features] spike` block in Cargo.toml plus this declaration.
#[cfg(feature = "spike")]
pub mod agent_graph_spike;

#[path = "util"]
pub mod util {
    pub mod formatting;
    pub use formatting::*;
}

#[path = "session"]
pub mod session {
    pub mod intent;
    pub mod parser;
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
