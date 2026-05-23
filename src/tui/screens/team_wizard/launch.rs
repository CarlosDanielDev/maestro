//! Launch flow — TeamPicker → InputPicker → PlanPreview → Confirm.

use super::issue_paste::parse_pasted_issue_token;
use super::launch_plan::{map_preflight_failure, plan_from_scheduler, plan_issue_count};
use super::types::{LaunchInputKind, LaunchStep, PreflightBlock, PreflightSummary};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::cost::estimate_cost_usd;
use crate::orchestration::dag::IssueState;
use crate::orchestration::preflight::preflight_sync;
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::types::TeamInput;
use crossterm::event::{KeyCode, KeyModifiers};

const INPUT_KINDS: &[LaunchInputKind] = &[
    LaunchInputKind::Issue,
    LaunchInputKind::IssueSet,
    LaunchInputKind::Milestone,
    LaunchInputKind::IdeaInbox,
];

impl TeamWizardScreen {
    pub(super) fn handle_launch(&mut self, code: KeyCode, modifiers: KeyModifiers) -> ScreenAction {
        // #875 — Ctrl+V paste on IssuePicker, regardless of autocomplete focus.
        if self.launch_step() == LaunchStep::IssuePicker
            && modifiers.contains(KeyModifiers::CONTROL)
            && code == KeyCode::Char('v')
        {
            if let Some(txt) = self.clipboard.read()
                && let Some(token) = parse_pasted_issue_token(&txt)
            {
                self.launch.manual_issue_input = token;
                self.launch.autocomplete_focus = None;
            }
            return ScreenAction::None;
        }

        if matches!(code, KeyCode::Esc) {
            // #877 — terminal states either swallow Esc (Executing — orphans
            // the background task otherwise) or treat it as "close wizard"
            // (LaunchSuccess / LaunchFailed). Retreating from a terminal
            // step would walk the user backward into Executing, leaving the
            // wizard stuck with no path forward.
            match self.launch_step() {
                LaunchStep::Executing => return ScreenAction::None,
                LaunchStep::LaunchSuccess | LaunchStep::LaunchFailed => {
                    return ScreenAction::Pop;
                }
                _ => return self.handle_launch_back(),
            }
        }
        if matches!(
            (self.launch_step(), code),
            (LaunchStep::LaunchSuccess, KeyCode::Enter)
        ) {
            return ScreenAction::Pop;
        }
        match (self.launch_step(), code) {
            (LaunchStep::TeamPicker, KeyCode::Up | KeyCode::Char('k')) => {
                self.launch_team_focus_dec()
            }
            (LaunchStep::TeamPicker, KeyCode::Down | KeyCode::Char('j')) => {
                self.launch_team_focus_inc()
            }
            (LaunchStep::TeamPicker, KeyCode::Enter) => self.launch_commit_team(),
            (LaunchStep::InputPicker, KeyCode::Up | KeyCode::Char('k')) => {
                self.launch_input_focus_dec()
            }
            (LaunchStep::InputPicker, KeyCode::Down | KeyCode::Char('j')) => {
                self.launch_input_focus_inc()
            }
            (LaunchStep::InputPicker, KeyCode::Enter) => self.launch_commit_input(),
            // #876 — autocomplete focus on IssuePicker
            (LaunchStep::IssuePicker, KeyCode::Down) => {
                self.launch_autocomplete_focus_inc();
            }
            (LaunchStep::IssuePicker, KeyCode::Up) => {
                self.launch_autocomplete_focus_dec();
            }
            (LaunchStep::IssuePicker, KeyCode::Tab) => {
                self.launch_autocomplete_commit();
            }
            (LaunchStep::IssuePicker, KeyCode::Backspace) => {
                self.launch.manual_issue_input.pop();
                self.launch.autocomplete_focus = None;
            }
            (LaunchStep::IssuePicker, KeyCode::Char(c))
                if c.is_ascii_digit() && self.launch.manual_issue_input.len() < 10 =>
            {
                self.launch.manual_issue_input.push(c);
                self.launch.autocomplete_focus = None;
            }
            (LaunchStep::IssuePicker, KeyCode::Enter) => self.launch_commit_issue_number(),
            (LaunchStep::PlanPreview, KeyCode::Enter) => self.launch_confirm_plan(),
            (LaunchStep::Confirm, KeyCode::Enter) => return self.launch_dispatch(),
            (LaunchStep::LaunchFailed, KeyCode::Char('r')) => {
                self.launch_step = LaunchStep::PlanPreview;
                self.failure_reason = None;
            }
            _ => {}
        }
        ScreenAction::None
    }

    fn launch_autocomplete_focus_inc(&mut self) {
        let candidates = self.autocomplete_candidates();
        if candidates.is_empty() {
            return;
        }
        let next = match self.launch.autocomplete_focus {
            None => 0,
            Some(i) if i + 1 < candidates.len() => i + 1,
            Some(i) => i,
        };
        self.launch.autocomplete_focus = Some(next);
    }

    fn launch_autocomplete_focus_dec(&mut self) {
        if self.autocomplete_candidates().is_empty() {
            return;
        }
        if let Some(i) = self.launch.autocomplete_focus {
            self.launch.autocomplete_focus = Some(i.saturating_sub(1));
        }
    }

    fn launch_autocomplete_commit(&mut self) {
        let candidates = self.autocomplete_candidates();
        let Some(idx) = self.launch.autocomplete_focus else {
            return;
        };
        if let Some(n) = candidates.get(idx) {
            self.launch.manual_issue_input = n.to_string();
            self.launch.autocomplete_focus = None;
        }
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
                LaunchInputKind::Issue => None,
                LaunchInputKind::IssueSet | LaunchInputKind::Milestone => {
                    if self.launch.manual_issues.is_empty() {
                        Some("No issues selected")
                    } else {
                        None
                    }
                }
                LaunchInputKind::IdeaInbox => None,
            },
            LaunchStep::IssuePicker => {
                let trimmed = self.launch.manual_issue_input.trim();
                if trimmed.is_empty() {
                    return Some("Enter an issue number");
                }
                match trimmed.parse::<u64>() {
                    Ok(n) if n > 0 => None,
                    Ok(_) => Some("Issue number must be greater than 0"),
                    Err(_) => Some("Enter a valid issue number"),
                }
            }
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
        let Some(kind) = INPUT_KINDS.get(self.launch.input_focus).copied() else {
            return;
        };
        self.launch.input_kind = kind;
        match kind {
            LaunchInputKind::Issue => {
                self.launch_step = LaunchStep::IssuePicker;
            }
            LaunchInputKind::IssueSet | LaunchInputKind::Milestone => {
                if self.launch.manual_issues.is_empty() {
                    return;
                }
                self.launch_step = LaunchStep::PlanPreview;
                self.build_plan_preview();
            }
            LaunchInputKind::IdeaInbox => {
                self.launch_step = LaunchStep::PlanPreview;
                self.build_plan_preview();
            }
        }
    }

    fn launch_commit_issue_number(&mut self) {
        let trimmed = self.launch.manual_issue_input.trim();
        if let Ok(n) = trimmed.parse::<u64>()
            && n > 0
        {
            self.launch.manual_issues = vec![n];
            self.try_advance();
        }
    }

    fn launch_confirm_plan(&mut self) {
        if let Some(Ok(())) = &self.launch.preflight {
            self.launch_step = LaunchStep::Confirm;
        }
    }

    /// Build the LaunchTeam action and transition to Executing. The
    /// dispatcher (`screen_dispatch.rs`) re-resolves the team from the
    /// wizard's `resolved_teams` cache, builds the Scheduler, and fans the
    /// run out per-level. The wizard listens for `apply_launch_result`.
    fn launch_dispatch(&mut self) -> ScreenAction {
        let Some(team_name) = self.launch.selected_team.clone() else {
            return ScreenAction::None;
        };
        if !self.resolved_teams.contains_key(&team_name) {
            return ScreenAction::None;
        }
        let team_input = match self.launch.input_kind {
            LaunchInputKind::Issue => match self.launch.manual_issue() {
                Some(n) => TeamInput::Issue { number: n },
                None => return ScreenAction::None,
            },
            LaunchInputKind::IssueSet | LaunchInputKind::Milestone => TeamInput::IssueSet {
                primary_milestone: self.launch.primary_milestone,
                issues: self.launch.manual_issues.clone(),
            },
            LaunchInputKind::IdeaInbox => TeamInput::IdeaInbox,
        };
        self.launch_step = LaunchStep::Executing;
        ScreenAction::LaunchTeam {
            team_name,
            input: team_input,
            max_parallel: self.launch.max_parallel.max(1),
        }
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
