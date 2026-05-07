//! Launch flow — TeamPicker → InputPicker → PlanPreview → Confirm.

use super::types::{LaunchInputKind, LaunchStep, PlanPreview, PreflightBlock, PreflightSummary};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::cost::estimate_cost_usd;
use crate::orchestration::dag::IssueState;
use crate::orchestration::preflight::{PreflightFailure, preflight_sync};
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::types::TeamInput;
use crate::orchestration::validation::ValidationError;
use crossterm::event::KeyCode;

const INPUT_KINDS: &[LaunchInputKind] = &[
    LaunchInputKind::Issue,
    LaunchInputKind::IssueSet,
    LaunchInputKind::Milestone,
    LaunchInputKind::IdeaInbox,
];

impl TeamWizardScreen {
    pub(super) fn handle_launch(&mut self, code: KeyCode) -> ScreenAction {
        if matches!(code, KeyCode::Esc) {
            return self.handle_launch_back();
        }
        if matches!(
            (self.launch_step(), code),
            (LaunchStep::LaunchSuccess, KeyCode::Enter)
        ) {
            return ScreenAction::Pop;
        }
        match (self.launch_step(), code) {
            (LaunchStep::TeamPicker, KeyCode::Up) => self.launch_team_focus_dec(),
            (LaunchStep::TeamPicker, KeyCode::Down) => self.launch_team_focus_inc(),
            (LaunchStep::TeamPicker, KeyCode::Enter) => self.launch_commit_team(),
            (LaunchStep::InputPicker, KeyCode::Up) => self.launch_input_focus_dec(),
            (LaunchStep::InputPicker, KeyCode::Down) => self.launch_input_focus_inc(),
            (LaunchStep::InputPicker, KeyCode::Enter) => self.launch_commit_input(),
            (LaunchStep::PlanPreview, KeyCode::Enter) => self.launch_confirm_plan(),
            (LaunchStep::Confirm, KeyCode::Enter) => self.launch_dispatch(),
            (LaunchStep::LaunchFailed, KeyCode::Char('r')) => {
                self.launch_step = LaunchStep::PlanPreview;
                self.failure_reason = None;
            }
            _ => {}
        }
        ScreenAction::None
    }

    pub(super) fn handle_launch_back(&mut self) -> ScreenAction {
        if self.launch_step.is_first() {
            self.switch_mode(TeamWizardMode::Home);
        } else {
            self.retreat();
        }
        ScreenAction::None
    }

    pub(super) fn validate_launch_step(&self) -> Option<&'static str> {
        match self.launch_step {
            LaunchStep::TeamPicker => {
                if self.launch.selected_team.is_some() {
                    None
                } else {
                    Some("Select a team")
                }
            }
            LaunchStep::InputPicker => match self.launch.input_kind {
                LaunchInputKind::Issue => {
                    if self.launch.manual_issue().is_some() {
                        None
                    } else {
                        Some("Issue not selected")
                    }
                }
                LaunchInputKind::IssueSet | LaunchInputKind::Milestone => {
                    if self.launch.manual_issues.is_empty() {
                        Some("No issues selected")
                    } else {
                        None
                    }
                }
                LaunchInputKind::IdeaInbox => None,
            },
            LaunchStep::PlanPreview => match &self.launch.preflight {
                Some(Ok(())) => None,
                Some(Err(_)) => Some("Pre-flight failed — fix blockers"),
                None => Some("Pre-flight not run"),
            },
            _ => None,
        }
    }

    fn launch_team_focus_inc(&mut self) {
        let max = self.resolved_teams.len().saturating_sub(1);
        if self.launch.team_focus < max {
            self.launch.team_focus += 1;
        }
    }

    fn launch_team_focus_dec(&mut self) {
        self.launch.team_focus = self.launch.team_focus.saturating_sub(1);
    }

    fn launch_commit_team(&mut self) {
        let mut names: Vec<&str> = self.resolved_teams.keys().map(String::as_str).collect();
        names.sort();
        if let Some(name) = names.get(self.launch.team_focus) {
            self.launch.selected_team = Some((*name).to_string());
            self.try_advance();
        }
    }

    fn launch_input_focus_inc(&mut self) {
        let max = INPUT_KINDS.len().saturating_sub(1);
        if self.launch.input_focus < max {
            self.launch.input_focus += 1;
        }
    }

    fn launch_input_focus_dec(&mut self) {
        self.launch.input_focus = self.launch.input_focus.saturating_sub(1);
    }

    fn launch_commit_input(&mut self) {
        if let Some(kind) = INPUT_KINDS.get(self.launch.input_focus) {
            self.launch.input_kind = *kind;
            self.try_advance();
        }
    }

    fn launch_confirm_plan(&mut self) {
        if let Some(Ok(())) = &self.launch.preflight {
            self.launch_step = LaunchStep::Confirm;
        }
    }

    fn launch_dispatch(&mut self) {
        self.launch_step = LaunchStep::Executing;
    }

    pub fn apply_launch_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.launch_step = LaunchStep::LaunchSuccess;
                self.failure_reason = None;
            }
            Err(e) => {
                self.launch_step = LaunchStep::LaunchFailed;
                self.failure_reason = Some(e);
            }
        }
    }

    /// Build the plan preview for the currently selected team and input.
    /// Stores the plan + preflight summary on `self.launch`.
    pub(super) fn build_plan_preview(&mut self) {
        let Some(team_name) = self.launch.selected_team.clone() else {
            return;
        };
        let Some(team) = self.resolved_teams.get(&team_name).cloned() else {
            return;
        };

        let original_count = self.launch.manual_issues.len();
        let team_input = match self.launch.input_kind {
            LaunchInputKind::Issue => match self.launch.manual_issue() {
                Some(n) => TeamInput::Issue { number: n },
                None => return,
            },
            LaunchInputKind::IssueSet => TeamInput::IssueSet {
                primary_milestone: self.launch.primary_milestone,
                issues: self.launch.manual_issues.clone(),
            },
            LaunchInputKind::Milestone => TeamInput::IssueSet {
                primary_milestone: self.launch.primary_milestone,
                issues: self.launch.manual_issues.clone(),
            },
            LaunchInputKind::IdeaInbox => TeamInput::IdeaInbox,
        };

        let metas = self.issue_metas.clone();
        let preview = match Scheduler::from_input(
            team.clone(),
            team_input,
            metas,
            self.launch.max_parallel.max(1),
        ) {
            Ok(scheduler) => Some(plan_from_scheduler(
                &scheduler,
                original_count,
                estimate_cost_usd(&team, plan_issue_count(&scheduler), 200),
            )),
            Err(e) => {
                self.failure_reason = Some(format!("Scheduler error: {e}"));
                None
            }
        };
        self.launch.plan = preview;

        let preflight = self.compute_preflight(&team);
        self.launch.preflight = Some(preflight);
    }

    fn compute_preflight(
        &self,
        team: &crate::orchestration::team::ResolvedTeam,
    ) -> Result<(), PreflightSummary> {
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

        for issue in &self.launch.manual_issues {
            let Some(meta) = self.issue_metas.get(issue) else {
                continue;
            };
            for dep in &meta.blocked_by {
                if let Some(dep_meta) = self.issue_metas.get(dep)
                    && dep_meta.state == IssueState::Open
                    && !self.launch.manual_issues.contains(dep)
                {
                    summary.blocking.push(PreflightBlock::OpenExternalDep {
                        issue: *issue,
                        ext_dep: *dep,
                    });
                }
            }
        }

        if summary.blocking.is_empty() {
            Ok(())
        } else {
            Err(summary)
        }
    }
}

fn map_preflight_failure(failure: PreflightFailure) -> Vec<PreflightBlock> {
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

fn plan_from_scheduler(scheduler: &Scheduler, original_count: usize, cost_usd: f64) -> PlanPreview {
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

fn plan_issue_count(scheduler: &Scheduler) -> usize {
    scheduler.run.plan.iter().map(|l| l.len()).sum()
}
