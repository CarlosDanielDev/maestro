//! GitHub provider integration.
//!
//! Contains the `gh` CLI client, typed API models, and helpers for
//! CI polling, PR creation/merge, and label management.

pub mod ci;
pub mod client;
pub mod gh_argv;
pub mod labels;
pub mod merge;
pub mod pr;
pub mod types;
