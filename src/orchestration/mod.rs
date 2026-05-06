//! Multi-agent team orchestration foundation.
//!
//! See `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md` §4.

#![allow(dead_code)]

pub(crate) mod builtins;
pub mod contracts;
pub mod loader;
pub mod team;
pub mod types;
pub mod validation;

#[allow(unused_imports)]
pub use contracts::{
    Finding, FindingSeverity, NewIssueDraft, ReviewVerdict, SubagentError, SubagentResult,
};
#[allow(unused_imports)]
pub use loader::Loader;
#[allow(unused_imports)]
pub use team::{ResolvedTeam, RoleBinding, RoleOverride, SourceTier, TeamConfig};
#[allow(unused_imports)]
pub use types::{Primitive, TeamInput, TeamOutput, TeamRole};
