//! Shared fakes / builders for team_wizard unit + snapshot tests.

#![allow(dead_code)]

use crate::agent_provider::types::{AgentHealthCheck, AgentProviderId};
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
use crate::orchestration::types::{Primitive, TeamRole};
use std::collections::HashMap;

pub fn make_test_team(
    name: &str,
    primitive: Primitive,
    bindings: &[(TeamRole, &str)],
    tier: SourceTier,
) -> ResolvedTeam {
    let mut b = HashMap::new();
    for (role, agent) in bindings {
        b.insert(
            *role,
            RoleBinding {
                agent: (*agent).into(),
                ..Default::default()
            },
        );
    }
    ResolvedTeam {
        name: name.into(),
        primitive,
        min_agents: vec!["claude".into()],
        bindings: b,
        source_tier: tier,
    }
}

pub fn make_health_check(agent_id: &str, available: bool) -> AgentHealthCheck {
    AgentHealthCheck {
        provider_id: AgentProviderId::new(agent_id),
        available,
        version: if available {
            Some("1.0.0".into())
        } else {
            None
        },
        message: if available {
            "ok".into()
        } else {
            "not installed".into()
        },
    }
}

pub fn make_issue_meta(
    number: u64,
    state: IssueState,
    milestone: Option<u64>,
    blocked_by: &[u64],
) -> IssueMeta {
    IssueMeta {
        number,
        state,
        milestone,
        blocked_by: blocked_by.to_vec(),
    }
}
