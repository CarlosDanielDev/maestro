//! Load-time validation per spec §4 "Validation rules".

#![allow(dead_code)]

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::TeamRole;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("team {team:?}: missing required role {role:?} for primitive {primitive:?}")]
    MissingRequiredRole {
        team: String,
        role: TeamRole,
        primitive: String,
    },
    #[error(
        "team {team:?}: agent {agent:?} (referenced by role {role:?}) is not configured in [agents.*]"
    )]
    AgentNotConfigured {
        team: String,
        agent: String,
        role: TeamRole,
    },
    #[error("team {team:?}: mode {mode:?} (role {role:?}) is not configured in [modes.*]")]
    ModeNotConfigured {
        team: String,
        mode: String,
        role: TeamRole,
    },
    #[error("team {team:?}: claude must be in min_agents (L2 provider constraint, see spec §3)")]
    ClaudeNotInMinAgents { team: String },
}

/// Validate a resolved team against the live config (known agents, modes).
pub fn validate(
    team: &ResolvedTeam,
    known_agents: &[String],
    known_modes: &[String],
) -> Result<(), Vec<ValidationError>> {
    let mut errs = Vec::new();

    // Required-roles check.
    for role in team.primitive.required_roles() {
        if !team.bindings.contains_key(role) {
            errs.push(ValidationError::MissingRequiredRole {
                team: team.name.clone(),
                role: *role,
                primitive: format!("{:?}", team.primitive).to_lowercase(),
            });
        }
    }

    // Agent existence.
    for (role, binding) in &team.bindings {
        if !known_agents.iter().any(|a| a == &binding.agent) {
            errs.push(ValidationError::AgentNotConfigured {
                team: team.name.clone(),
                agent: binding.agent.clone(),
                role: *role,
            });
        }
        if let Some(mode) = &binding.mode
            && !known_modes.iter().any(|m| m == mode)
        {
            errs.push(ValidationError::ModeNotConfigured {
                team: team.name.clone(),
                mode: mode.clone(),
                role: *role,
            });
        }
    }

    // L2 Claude constraint (spec §3).
    if !team.min_agents.iter().any(|a| a == "claude") {
        errs.push(ValidationError::ClaudeNotInMinAgents {
            team: team.name.clone(),
        });
    }

    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::Primitive;
    use std::collections::HashMap;

    fn binding(agent: &str) -> RoleBinding {
        RoleBinding {
            agent: agent.into(),
            mode: None,
            model_override: None,
            prompt_addendum: None,
            fallback_agent: None,
        }
    }

    #[test]
    fn pipeline_missing_reviewer_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(TeamRole::Implementer, binding("claude"));
        bindings.insert(TeamRole::Docs, binding("claude"));
        let team = ResolvedTeam {
            name: "broken".into(),
            primitive: Primitive::Pipeline,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["claude".into()], &[]).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::MissingRequiredRole {
                role: TeamRole::Reviewer,
                ..
            }
        )));
    }

    #[test]
    fn unknown_agent_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(TeamRole::Reviewer, binding("ghost"));
        let team = ResolvedTeam {
            name: "ghost-team".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["claude".into()], &[]).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::AgentNotConfigured { .. }))
        );
    }

    #[test]
    fn missing_claude_in_min_agents_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(TeamRole::Reviewer, binding("ollama"));
        let team = ResolvedTeam {
            name: "no-claude".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["ollama".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["ollama".into()], &[]).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::ClaudeNotInMinAgents { .. }))
        );
    }

    #[test]
    fn unknown_mode_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(
            TeamRole::Reviewer,
            RoleBinding {
                agent: "claude".into(),
                mode: Some("ghost".into()),
                model_override: None,
                prompt_addendum: None,
                fallback_agent: None,
            },
        );
        let team = ResolvedTeam {
            name: "bad-mode".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["claude".into()], &[]).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::ModeNotConfigured { .. }))
        );
    }
}
