//! The ratatui frontend (ADR 006): dashboard, screens, widgets and the
//! mascot. Moved here from the root crate's `src/tui/` (#1306).
//!
//! Core is reached only through `maestro-core`; the `pub(crate) use` block
//! below keeps the `crate::config`, `crate::session`, ... paths the TUI has
//! always used, so the move stays a pure relocation.
//!
//! Crate-wide lint policy lives in the workspace `Cargo.toml` under
//! `[workspace.lints]`; see `docs/RUST-GUARDRAILS.md` for the full policy.

#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::significant_drop_tightening)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::branches_sharing_code)]

pub mod mascot;
pub mod setup;
pub mod tui;

#[cfg(test)]
mod integration_tests;

pub(crate) use maestro_core::adapt;
pub(crate) use maestro_core::agent_provider;
pub(crate) use maestro_core::agent_provider_map;
pub(crate) use maestro_core::budget;
pub(crate) use maestro_core::changelog;
pub(crate) use maestro_core::config;
pub(crate) use maestro_core::continuous;
pub(crate) use maestro_core::flags;
pub(crate) use maestro_core::gates;
pub(crate) use maestro_core::git;
pub(crate) use maestro_core::icon_mode;
pub(crate) use maestro_core::icons;
pub(crate) use maestro_core::init;
pub(crate) use maestro_core::milestone_health;
pub(crate) use maestro_core::models;
pub(crate) use maestro_core::modes;
pub(crate) use maestro_core::notifications;
pub(crate) use maestro_core::orchestration;
pub(crate) use maestro_core::plugins;
pub(crate) use maestro_core::prd;
pub(crate) use maestro_core::prompts;
pub(crate) use maestro_core::provider;
pub(crate) use maestro_core::review;
pub(crate) use maestro_core::sanitize;
pub(crate) use maestro_core::session;
pub(crate) use maestro_core::settings;
pub(crate) use maestro_core::state;
pub(crate) use maestro_core::system;
pub(crate) use maestro_core::turboquant;
pub(crate) use maestro_core::updater;
pub(crate) use maestro_core::util;
pub(crate) use maestro_core::work;
