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

pub use maestro_core::icon_mode;
pub use maestro_core::icons;
pub use maestro_core::turboquant;

pub use maestro_cli::cli;
pub use maestro_core::adapt;
pub use maestro_core::agent_provider;
pub use maestro_core::budget;
pub use maestro_core::changelog;
pub mod commands;
pub use maestro_core::config;
pub use maestro_core::continuous;
pub use maestro_core::doctor;
pub use maestro_core::flags;
pub use maestro_core::gates;
pub use maestro_core::git;
pub use maestro_core::init;
pub use maestro_core::insight;
pub mod mascot;
pub use maestro_core::milestone_health;
pub use maestro_core::models;
pub use maestro_core::modes;
pub use maestro_core::notifications;
pub use maestro_core::orchestration;
pub use maestro_core::plugins;
pub use maestro_core::prd;
pub use maestro_core::prompts;
pub use maestro_core::provider;
pub use maestro_core::review;
pub use maestro_core::sanitize;
pub use maestro_core::session;
pub use maestro_core::settings;
pub use maestro_core::state;
pub use maestro_core::system;
pub use maestro_core::templates;
pub mod tui;
pub use maestro_core::updater;
pub use maestro_core::util;
pub use maestro_core::work;
