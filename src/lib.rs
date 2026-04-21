//! Library facade — exposes only self-contained modules for benchmarks.
//!
//! Crate-wide lint policy lives in `Cargo.toml` under `[lints]`; see
//! `docs/RUST-GUARDRAILS.md` for the full policy document.

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
    pub mod transition;
    pub mod types;
}
