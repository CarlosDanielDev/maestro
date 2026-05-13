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

pub mod adapt;
pub mod agent_provider;
pub mod budget;
pub mod changelog;
pub mod cli;
pub mod commands;
pub mod config;
pub mod continuous;
pub mod doctor;
pub mod flags;
pub mod gates;
pub mod git;
pub mod init;
pub mod mascot;
pub mod milestone_health;
pub mod models;
pub mod modes;
pub mod notifications;
pub mod orchestration;
pub mod plugins;
pub mod prd;
pub mod prompts;
pub mod provider;
pub mod review;
pub mod sanitize;
pub mod session;
pub mod settings;
pub mod state;
pub mod system;
pub mod templates;
pub mod tui;
pub mod updater;
pub mod util;
pub mod work;
