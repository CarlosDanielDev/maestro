//! Shared maestro logic used by every frontend (ADR 006).
//!
//! Session pool, orchestration, state, providers, budget, git and config.
//! Nothing here may import the TUI or the CLI: the crate does not depend on
//! `maestro-tui` or `maestro-cli`, so the compiler holds the boundary.
//!
//! Crate-wide lint policy lives in the workspace `Cargo.toml` under
//! `[workspace.lints]`; see `docs/RUST-GUARDRAILS.md` for the full policy.

#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::significant_drop_tightening)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::branches_sharing_code)]

pub mod adapt;
pub mod agent_provider;
pub mod budget;
pub mod changelog;
pub mod config;
pub mod continuous;
pub mod doctor;
pub mod flags;
pub mod gates;
pub mod git;
pub mod icon_mode;
pub mod icons;
pub mod init;
pub mod insight;
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
pub mod turboquant;
pub mod updater;
pub mod util;
pub mod work;

/// Some core tests read repo files through cwd-relative paths
/// (`.claude/skills`, `.maestro/templates`), the way production code does.
/// They ran from the repo root while core lived in the root package; cargo
/// now runs them from `crates/maestro-core`, so they opt back in here.
/// `maestro-cli` tests use it too (via `test-util`, #1307).
#[cfg(any(test, feature = "test-util"))]
pub fn enter_repo_root_for_test() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .expect("repo root must exist");
    });
}
