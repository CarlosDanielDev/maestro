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
    pub mod transition;
    pub mod types;
}
