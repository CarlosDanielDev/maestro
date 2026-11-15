//! Pre-flight validation pipeline. Async doctor hook lands in a later chunk.

#![allow(dead_code)]

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::validation::{ValidationError, validate};

#[derive(Debug, Clone, PartialEq)]
pub enum PreflightFailure {
    Validation(Vec<ValidationError>),
    L2ProviderUnavailable,
    AgentUnhealthy { id: String, reason: String },
    DagCycle(String),
    MalformedBlockedBy { issue: u64, snippet: String },
}

/// Sync portion of pre-flight. The async health-check portion wires in later.
pub fn preflight_sync(
    team: &ResolvedTeam,
    known_agents: &[String],
    known_modes: &[String],
) -> Result<(), PreflightFailure> {
    if let Err(errs) = validate(team, known_agents, known_modes) {
        return Err(PreflightFailure::Validation(errs));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::{Primitive, TeamRole};
    use std::collections::HashMap;

    #[test]
    fn passes_for_valid_team() {
        let mut bindings = HashMap::new();
        bindings.insert(
            TeamRole::Reviewer,
            RoleBinding {
                agent: "claude".into(),
                mode: None,
                model_override: None,
                prompt_addendum: None,
                fallback_agent: None,
            },
        );
        let team = ResolvedTeam {
            name: "default-reviewer".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        };
        assert!(preflight_sync(&team, &["claude".into()], &[]).is_ok());
    }
}
