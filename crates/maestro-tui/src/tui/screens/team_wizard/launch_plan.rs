//! Pure helpers for building the Launch plan preview from a Scheduler run
//! and translating preflight-failure types into wizard-side `PreflightBlock`
//! entries. Extracted from `launch.rs` to keep that file under the 400-LOC
//! cap.

use super::types::{PlanPreview, PreflightBlock};
use crate::orchestration::preflight::PreflightFailure;
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::validation::ValidationError;

pub(super) fn map_preflight_failure(failure: PreflightFailure) -> Vec<PreflightBlock> {
    match failure {
        PreflightFailure::Validation(errs) => {
            errs.into_iter().filter_map(map_validation_error).collect()
        }
        PreflightFailure::AgentUnhealthy { id, reason } => vec![PreflightBlock::AgentUnhealthy {
            agent_id: id,
            message: reason,
        }],
        PreflightFailure::L2ProviderUnavailable => vec![PreflightBlock::AgentUnhealthy {
            agent_id: "claude".into(),
            message: "L2 provider unavailable".into(),
        }],
        PreflightFailure::DagCycle(_) | PreflightFailure::MalformedBlockedBy { .. } => Vec::new(),
    }
}

fn map_validation_error(err: ValidationError) -> Option<PreflightBlock> {
    match err {
        ValidationError::MissingRequiredRole { role, .. } => {
            Some(PreflightBlock::MissingRoleBinding { role })
        }
        ValidationError::AgentNotConfigured { agent, .. } => Some(PreflightBlock::AgentUnhealthy {
            agent_id: agent,
            message: "agent not configured".into(),
        }),
        ValidationError::ModeNotConfigured { mode, .. } => Some(PreflightBlock::AgentUnhealthy {
            agent_id: mode,
            message: "mode not configured".into(),
        }),
        ValidationError::ClaudeNotInMinAgents { .. } => {
            Some(PreflightBlock::MissingClaudeInMinAgents)
        }
    }
}

pub(super) fn plan_from_scheduler(
    scheduler: &Scheduler,
    original_count: usize,
    cost_usd: f64,
) -> PlanPreview {
    let levels: Vec<Vec<u64>> = scheduler.run.plan.clone();
    let final_count: usize = levels.iter().map(|l| l.len()).sum();
    PlanPreview {
        team_name: scheduler.team.name.clone(),
        primitive: scheduler.team.primitive,
        levels,
        auto_added: scheduler.auto_added.clone(),
        original_count,
        final_count,
        estimated_cost_usd: cost_usd,
        max_parallel: scheduler.max_parallel,
    }
}

pub(super) fn plan_issue_count(scheduler: &Scheduler) -> usize {
    scheduler.run.plan.iter().map(|l| l.len()).sum()
}
