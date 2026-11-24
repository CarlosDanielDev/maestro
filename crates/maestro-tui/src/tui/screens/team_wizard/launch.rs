//! Launch flow (#1290, §02.17): Team · Input · Plan & launch, then the
//! launch dialog, the run and its result as `LaunchPhase`s.

use super::issue_paste::parse_pasted_issue_token;
use super::overview::ordered_presets;
use super::types::{LaunchInputKind, LaunchPhase, LaunchStep};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::types::TeamInput;
use crossterm::event::{Event, KeyCode, KeyModifiers};

pub(super) const INPUT_KINDS: &[LaunchInputKind] = &[
    LaunchInputKind::Issue,
    LaunchInputKind::IssueSet,
    LaunchInputKind::Milestone,
    LaunchInputKind::IdeaInbox,
];

/// Most issues the find field suggests.
const MAX_MATCHES: usize = 5;

impl TeamWizardScreen {
    pub(super) fn handle_launch(
        &mut self,
        event: &Event,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> ScreenAction {
        self.launch.error = None;
        match self.launch_phase {
            // #877 / #1246: Esc cancels the run (leaving would orphan the
            // background task); every other key is swallowed.
            LaunchPhase::Executing => {
                if code == KeyCode::Esc {
                    return self.cancel_team_run();
                }
            }
            LaunchPhase::Launched => {
                if matches!(code, KeyCode::Enter | KeyCode::Esc) {
                    return ScreenAction::Pop;
                }
            }
            LaunchPhase::Failed => match code {
                KeyCode::Esc => return ScreenAction::Pop,
                // Back to the plan; nothing spends until the dialog again.
                KeyCode::Char('r') => {
                    self.launch_phase = LaunchPhase::Editing;
                    self.launch_step = LaunchStep::Plan;
                    self.failure_reason = None;
                    self.build_plan_preview();
                }
                _ => {}
            },
            LaunchPhase::Dialog => match code {
                KeyCode::Enter => return self.launch_dispatch(),
                KeyCode::Esc => self.launch_phase = LaunchPhase::Editing,
                _ => {}
            },
            LaunchPhase::Editing => return self.handle_launch_step(event, code, modifiers),
        }
        ScreenAction::None
    }

    fn handle_launch_step(
        &mut self,
        event: &Event,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> ScreenAction {
        if self.launch_step == LaunchStep::Input && self.launch.finding {
            self.handle_launch_find(event, code, modifiers);
            return ScreenAction::None;
        }
        match (self.launch_step, code) {
            (_, KeyCode::Esc) => return self.handle_launch_back(),
            (LaunchStep::Team, KeyCode::Up | KeyCode::Char('k')) => {
                self.launch.team_focus = self.launch.team_focus.saturating_sub(1);
            }
            (LaunchStep::Team, KeyCode::Down | KeyCode::Char('j')) => {
                let last = self.resolved_teams.len().saturating_sub(1);
                self.launch.team_focus = (self.launch.team_focus + 1).min(last);
            }
            (LaunchStep::Team, KeyCode::Enter) => self.launch_commit_team(),
            (LaunchStep::Input, KeyCode::Up | KeyCode::Char('k')) => {
                self.launch.input_focus = self.launch.input_focus.saturating_sub(1);
            }
            (LaunchStep::Input, KeyCode::Down | KeyCode::Char('j')) => {
                let last = INPUT_KINDS.len() - 1;
                self.launch.input_focus = (self.launch.input_focus + 1).min(last);
            }
            (LaunchStep::Input, KeyCode::Enter) => self.launch_commit_input(),
            (LaunchStep::Plan, KeyCode::Enter) => self.launch_open_dialog(),
            _ => {}
        }
        ScreenAction::None
    }

    /// The find-issue field (`widgets::input::Input`): every key it does
    /// not claim types into it.
    fn handle_launch_find(&mut self, event: &Event, code: KeyCode, modifiers: KeyModifiers) {
        // #875: ⌃V pastes `#n` or an issue URL as the number.
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('v') {
            if let Some(txt) = self.clipboard.read()
                && let Some(token) = parse_pasted_issue_token(&txt)
            {
                self.issue_input.set_text(&token);
                self.launch.autocomplete_focus = None;
            }
            return;
        }
        match code {
            KeyCode::Esc => self.launch.finding = false,
            KeyCode::Down => {
                let n = self.autocomplete_candidates().len();
                if n > 0 {
                    let next = self.launch.autocomplete_focus.map_or(0, |i| i + 1);
                    self.launch.autocomplete_focus = Some(next.min(n - 1));
                }
            }
            KeyCode::Up => {
                if let Some(i) = self.launch.autocomplete_focus {
                    self.launch.autocomplete_focus = Some(i.saturating_sub(1));
                }
            }
            KeyCode::Enter => self.launch_pick_issue(),
            _ => {
                if self.issue_input.input(event.clone()) {
                    self.launch.autocomplete_focus = None;
                }
            }
        }
    }

    /// The typed number, `#` allowed: `Err` names what is wrong.
    pub(super) fn typed_issue(&self) -> Result<u64, &'static str> {
        let text = self.issue_input.text();
        let trimmed = text.trim().trim_start_matches('#');
        if trimmed.is_empty() {
            return Err("Type an issue number");
        }
        match trimmed.parse::<u64>() {
            Ok(0) => Err("Issue number must be greater than 0"),
            Ok(n) => Ok(n),
            Err(_) => Err("Type an issue number, like 42 or #42"),
        }
    }

    /// Open issues whose number starts with the typed digits, ascending,
    /// at most five. Empty while the field is empty.
    pub fn autocomplete_candidates(&self) -> Vec<u64> {
        let text = self.issue_input.text();
        let prefix = text.trim().trim_start_matches('#');
        if prefix.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<u64> = (self.issue_metas.iter())
            .filter(|(_, m)| m.state == crate::orchestration::dag::IssueState::Open)
            .filter(|(n, _)| n.to_string().starts_with(prefix))
            .map(|(n, _)| *n)
            .collect();
        out.sort_unstable();
        out.truncate(MAX_MATCHES);
        out
    }

    /// `Enter` in the find field: the focused match, else the typed number.
    fn launch_pick_issue(&mut self) {
        let focused = (self.launch.autocomplete_focus)
            .and_then(|i| self.autocomplete_candidates().get(i).copied());
        let picked = match focused {
            Some(n) => Ok(n),
            None => self.typed_issue(),
        };
        match picked {
            Ok(n) => {
                self.issue_input.set_text(&n.to_string());
                self.launch.input_kind = LaunchInputKind::Issue;
                self.launch.manual_issues = vec![n];
                self.launch.finding = false;
                self.launch_go_to_plan();
            }
            Err(e) => self.launch.error = Some(e),
        }
    }

    pub(super) fn handle_launch_back(&mut self) -> ScreenAction {
        match self.launch_step.previous() {
            Some(prev) => self.launch_step = prev,
            None => self.switch_mode(TeamWizardMode::Home),
        }
        ScreenAction::None
    }

    /// Why the current step cannot move on; `None` when it can.
    pub(super) fn validate_launch_step(&self) -> Option<&'static str> {
        match self.launch_step {
            LaunchStep::Team => self
                .launch
                .selected_team
                .is_none()
                .then_some("Select a team"),
            LaunchStep::Input => match self.launch.input_kind {
                LaunchInputKind::Issue => self
                    .launch
                    .manual_issue()
                    .is_none()
                    .then_some("Pick an issue"),
                LaunchInputKind::IssueSet | LaunchInputKind::Milestone => {
                    self.launch.manual_issues.is_empty().then_some(
                        "No issues selected · open Launch from Milestones (t) to bring a set",
                    )
                }
                LaunchInputKind::IdeaInbox => Some(IDEA_INBOX_DISABLED),
            },
            LaunchStep::Plan => match (&self.launch.preflight, &self.launch.plan) {
                (Some(Err(_)), _) => Some("Pre-flight failed · fix the blockers first"),
                (None, _) => Some("Pre-flight not run"),
                (_, None) => Some("No plan · go back and pick the input"),
                (_, Some(p)) if p.final_count == 0 => {
                    Some("Nothing will run · every issue is held back")
                }
                _ => None,
            },
        }
    }

    fn launch_commit_team(&mut self) {
        let presets = ordered_presets(&self.resolved_teams);
        if let Some(team) = presets.get(self.launch.team_focus) {
            self.launch.selected_team = Some(team.name.clone());
            self.launch_step = LaunchStep::Input;
        } else {
            self.launch.error = Some("Select a team");
        }
    }

    fn launch_commit_input(&mut self) {
        let Some(kind) = INPUT_KINDS.get(self.launch.input_focus).copied() else {
            return;
        };
        match kind {
            LaunchInputKind::Issue => {
                self.launch.input_kind = kind;
                self.launch.finding = true;
            }
            LaunchInputKind::IdeaInbox => self.launch.error = Some(IDEA_INBOX_DISABLED),
            LaunchInputKind::IssueSet | LaunchInputKind::Milestone => {
                self.launch.input_kind = kind;
                self.launch_go_to_plan();
            }
        }
    }

    /// Move to Plan & launch (building the plan), or show why not.
    fn launch_go_to_plan(&mut self) {
        match self.validate_launch_step() {
            Some(e) => self.launch.error = Some(e),
            None => {
                self.launch_step = LaunchStep::Plan;
                self.build_plan_preview();
            }
        }
    }

    /// Plan `Enter`: open the launch dialog. It never spends by itself.
    fn launch_open_dialog(&mut self) {
        match self.validate_launch_step() {
            Some(e) => self.launch.error = Some(e),
            None => self.launch_phase = LaunchPhase::Dialog,
        }
    }

    /// Dialog `Enter`: dispatch exactly the issues the plan's waves hold.
    /// The dispatcher (`screen_dispatch.rs`) builds the Scheduler from
    /// them and `run_team` walks the waves; `apply_launch_result` answers.
    fn launch_dispatch(&mut self) -> ScreenAction {
        let Some(team_name) = self.launch.selected_team.clone() else {
            return ScreenAction::None;
        };
        if !self.resolved_teams.contains_key(&team_name) {
            return ScreenAction::None;
        }
        let Some(plan) = self.launch.plan.as_ref().filter(|p| p.final_count > 0) else {
            return ScreenAction::None;
        };
        let mut issues: Vec<u64> = plan.levels.iter().flatten().copied().collect();
        issues.sort_unstable();
        let input = match (self.launch.input_kind, issues.as_slice()) {
            (LaunchInputKind::Issue, [n]) => TeamInput::Issue { number: *n },
            _ => TeamInput::IssueSet {
                primary_milestone: self.launch.primary_milestone,
                issues,
            },
        };
        self.launch_phase = LaunchPhase::Executing;
        self.launch.running_wave = None;
        ScreenAction::LaunchTeam {
            team_name,
            input,
            max_parallel: self.launch.max_parallel.max(1),
        }
    }

    pub fn apply_launch_result(&mut self, result: Result<(), String>) {
        // The run ended: nothing left to cancel, and quitting no longer asks.
        self.launch.run_cancel = None;
        match result {
            Ok(()) => {
                self.launch_phase = LaunchPhase::Launched;
                self.failure_reason = None;
            }
            Err(e) => {
                self.launch_phase = LaunchPhase::Failed;
                self.failure_reason = Some(e);
            }
        }
    }
}

/// Out of scope until the triager input exists (§02.17 finding 5).
pub(super) const IDEA_INBOX_DISABLED: &str = "Idea inbox is not available yet · no triager input";
