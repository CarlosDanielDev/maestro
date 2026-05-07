//! Multi-agent team orchestration foundation.
//!
//! See `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md` §4.

#![allow(dead_code)]

pub(crate) mod builtins;
pub mod contracts;
pub mod dag;
pub mod loader;
pub mod orchestrator;
pub mod preflight;
pub mod primitives;
pub mod run;
pub mod scheduler;
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
pub use orchestrator::build_system_prompt;
#[allow(unused_imports)]
pub use primitives::{NextStep, PrimitiveMachine, PrimitiveOutput, make_machine};
#[allow(unused_imports)]
pub use team::{ResolvedTeam, RoleBinding, RoleOverride, SourceTier, TeamConfig};
#[allow(unused_imports)]
pub use types::{Primitive, TeamInput, TeamOutput, TeamRole};
