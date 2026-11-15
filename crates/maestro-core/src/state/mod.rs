//! Persistent session state.
//!
//! Manages **durable runtime data** that is written to disk (`maestro-state.json`)
//! and survives across process restarts: active sessions, cost totals, pending PRs,
//! file claims, progress tracking, and prompt history.
//!
//! This is intentionally separate from [`crate::flags`], which handles **ephemeral
//! feature toggles** resolved at startup from config and CLI arguments. State is
//! persisted; flags are not.

pub mod file_claims;
pub mod progress;
pub mod prompt_history;
pub mod store;
pub mod types;
