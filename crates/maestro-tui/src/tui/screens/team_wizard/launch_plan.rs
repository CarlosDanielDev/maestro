//! The Launch plan (#1290): the selection expanded by the Scheduler, then
//! planned by `work::run_plan`, plus the preflight and the translation of
//! preflight failures into wizard-side `PreflightBlock` entries.

use super::TeamWizardScreen;
use super::types::{LaunchInputKind, PlanPreview, PreflightBlock, PreflightSummary};
use crate::orchestration::preflight::{PreflightFailure, preflight_sync};
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::TeamInput;
use crate::orchestration::validation::ValidationError;
use crate::work::run_plan::run_plan;
use std::collections::{HashMap, HashSet};

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

impl TeamWizardScreen {
    /// The team input the selection stands for. `None` for an Issue input
    /// with no issue picked yet.
    pub(super) fn launch_team_input(&self) -> Option<TeamInput> {
        Some(match self.launch.input_kind {
            LaunchInputKind::Issue => TeamInput::Issue {
                number: self.launch.manual_issue()?,
            },
            LaunchInputKind::IssueSet | LaunchInputKind::Milestone => TeamInput::IssueSet {
                primary_milestone: self.launch.primary_milestone,
                issues: self.launch.manual_issues.clone(),
            },
            LaunchInputKind::IdeaInbox => TeamInput::IdeaInbox,
        })
    }

    /// Build the plan for the selected team and input. The Scheduler only
    /// expands the selection with its open blockers; the waves, the held
    /// issues and the worst case come from `work::run_plan` (#1290), the
    /// planner every other screen reads.
    pub(super) fn build_plan_preview(&mut self) {
        self.launch.plan = None;
        let Some(team_name) = self.launch.selected_team.clone() else {
            return;
        };
        let Some(team) = self.resolved_teams.get(&team_name).cloned() else {
            return;
        };
        let Some(team_input) = self.launch_team_input() else {
            return;
        };
        let original_count = self.launch.manual_issues.len();
        let max_parallel = self.launch.max_parallel.max(1);
        let metas = self.issue_metas.clone();
        let expanded = Scheduler::from_input(team.clone(), team_input, metas, max_parallel)
            .and_then(|scheduler| {
                let open: HashSet<u64> = scheduler.levels().iter().flatten().copied().collect();
                let plan = run_plan(
                    &open,
                    &self.issue_metas,
                    &HashSet::new(),
                    &HashMap::new(),
                    self.launch_budget.per_session_usd,
                )?;
                Ok((scheduler.auto_added, plan))
            });
        match expanded {
            Ok((auto_added, plan)) => {
                self.launch.plan = Some(PlanPreview {
                    team_name: team.name.clone(),
                    primitive: team.primitive,
                    final_count: plan.waves.iter().map(Vec::len).sum(),
                    levels: plan.waves,
                    deferred: plan.deferred,
                    auto_added,
                    original_count,
                    worst_case_usd: plan.worst_case_usd,
                    max_parallel,
                });
            }
            Err(e) => self.failure_reason = Some(format!("Scheduler error: {e}")),
        }
        let preflight = self.compute_preflight(&team);
        self.launch.preflight = Some(preflight);
    }

    fn compute_preflight(&self, team: &ResolvedTeam) -> Result<(), PreflightSummary> {
        let mut summary = PreflightSummary::default();

        if let Err(failure) = preflight_sync(team, &self.known_agents, &self.known_modes) {
            summary.blocking.extend(map_preflight_failure(failure));
        }

        for (id, health) in &self.preflight.by_agent {
            if !health.available {
                summary.blocking.push(PreflightBlock::AgentUnhealthy {
                    agent_id: id.clone(),
                    message: health.message.clone(),
                });
            }
        }

        if !team.min_agents.iter().any(|a| a == "claude") {
            summary
                .blocking
                .push(PreflightBlock::MissingClaudeInMinAgents);
        }

        // An issue blocked by an open issue outside the run is no longer a
        // preflight failure for the whole run (#1290): `run_plan` holds it
        // in NOT INCLUDED with the reason, and only the waves dispatch.

        if summary.blocking.is_empty() {
            Ok(())
        } else {
            Err(summary)
        }
    }
}
