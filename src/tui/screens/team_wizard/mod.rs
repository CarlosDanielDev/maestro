//! Team Wizard.
//!
//! Three flows in one screen:
//! - Compose: Source → Primitive → Roles → Overrides → Save
//! - Launch:  TeamPicker → InputPicker → PlanPreview → Confirm
//! - Manage:  list user-tier presets, edit (jumps into Compose), delete
//!
//! Mirrors the `milestone_wizard` step-machine pattern.

#![allow(dead_code)]

mod compose;
mod draw;
mod launch;
mod manage;
pub mod types;

#[cfg(test)]
pub mod test_helpers;

pub use types::{
    AgentHealth, ComposePayload, ComposeSource, ComposeStep, ComposeTier, LaunchInputKind,
    LaunchPayload, LaunchStep, ManageState, ManageStep, PreflightCache, TeamLaunchInput,
    TeamWizardMode,
};

use crate::orchestration::dag::IssueMeta;
use crate::orchestration::team::ResolvedTeam;
use crate::provider::types::ProviderKind;
use crate::state::types::IssueNumber;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};
use std::collections::HashMap;

use super::{Screen, ScreenAction};

/// Top-level wizard screen. Owns step machines for all three flows plus
/// session-scoped caches (resolved teams, health check, issue metas).
pub struct TeamWizardScreen {
    provider_kind: ProviderKind,
    mode: TeamWizardMode,
    compose_step: ComposeStep,
    launch_step: LaunchStep,
    manage_step: ManageStep,
    pub(super) compose: ComposePayload,
    pub(super) launch: LaunchPayload,
    pub(super) manage: ManageState,
    pub(super) preflight: PreflightCache,
    pub(super) resolved_teams: HashMap<String, ResolvedTeam>,
    pub(super) known_agents: Vec<String>,
    pub(super) known_modes: Vec<String>,
    pub(super) issue_metas: HashMap<IssueNumber, IssueMeta>,
    pub(super) initial_input: Option<TeamLaunchInput>,
    pub(super) spinner_tick: usize,
    pub(super) use_nerd_font: bool,
    pub(super) failure_reason: Option<String>,
}

impl TeamWizardScreen {
    pub fn new(provider_kind: ProviderKind) -> Self {
        Self::with_entry(provider_kind, TeamWizardMode::Home, None)
    }

    pub fn with_entry(
        provider_kind: ProviderKind,
        mode: TeamWizardMode,
        preselect: Option<TeamLaunchInput>,
    ) -> Self {
        let mut launch = LaunchPayload {
            max_parallel: 3,
            ..LaunchPayload::default()
        };
        let initial_input = preselect.clone();
        if let Some(input) = preselect {
            apply_initial_input(&mut launch, &input);
        }
        Self {
            provider_kind,
            mode,
            compose_step: ComposeStep::default(),
            launch_step: starting_launch_step(&initial_input),
            manage_step: ManageStep::default(),
            compose: ComposePayload::default(),
            launch,
            manage: ManageState::default(),
            preflight: PreflightCache::default(),
            resolved_teams: HashMap::new(),
            known_agents: Vec::new(),
            known_modes: Vec::new(),
            issue_metas: HashMap::new(),
            initial_input,
            spinner_tick: 0,
            use_nerd_font: false,
            failure_reason: None,
        }
    }

    pub fn set_spinner_context(&mut self, spinner_tick: usize, use_nerd_font: bool) {
        self.spinner_tick = spinner_tick;
        self.use_nerd_font = use_nerd_font;
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.provider_kind
    }

    pub fn mode(&self) -> TeamWizardMode {
        self.mode
    }

    pub fn compose_step(&self) -> ComposeStep {
        self.compose_step
    }

    pub fn launch_step(&self) -> LaunchStep {
        self.launch_step
    }

    pub fn manage_step(&self) -> ManageStep {
        self.manage_step
    }

    pub fn launch_payload(&self) -> &LaunchPayload {
        &self.launch
    }

    pub fn compose_payload(&self) -> &ComposePayload {
        &self.compose
    }

    pub fn manage_state(&self) -> &ManageState {
        &self.manage
    }

    pub fn resolved_teams(&self) -> &HashMap<String, ResolvedTeam> {
        &self.resolved_teams
    }

    pub fn known_agents(&self) -> &[String] {
        &self.known_agents
    }

    pub fn known_modes(&self) -> &[String] {
        &self.known_modes
    }

    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }

    pub fn initial_input(&self) -> Option<&TeamLaunchInput> {
        self.initial_input.as_ref()
    }

    /// Replace the resolved-teams cache. Idempotent — calling twice replaces.
    pub fn apply_resolved_teams(&mut self, teams: Vec<ResolvedTeam>) {
        self.resolved_teams.clear();
        for t in teams {
            self.resolved_teams.insert(t.name.clone(), t);
        }
    }

    /// Replace the health-check cache. Each entry's `available` flag drives
    /// `is_healthy()`; absent agents are treated as unhealthy (fail-closed).
    pub fn apply_health_check(
        &mut self,
        results: Vec<crate::agent_provider::types::AgentHealthCheck>,
    ) {
        self.preflight.by_agent.clear();
        for r in results {
            self.preflight.by_agent.insert(
                r.provider_id.as_str().to_string(),
                AgentHealth {
                    available: r.available,
                    version: r.version,
                    message: r.message,
                },
            );
        }
        self.preflight.fetched = true;
    }

    /// Replace the issue-meta cache used by Launch's plan preview.
    pub fn apply_issue_metas(&mut self, metas: HashMap<IssueNumber, IssueMeta>) {
        self.issue_metas = metas;
    }

    pub fn set_known_agents(&mut self, agents: Vec<String>) {
        self.known_agents = agents;
    }

    pub fn set_known_modes(&mut self, modes: Vec<String>) {
        self.known_modes = modes;
    }

    /// Health check predicate. Fail-closed when the cache is empty or the
    /// agent is absent.
    pub fn is_healthy(&self, agent_id: &str) -> bool {
        self.preflight
            .by_agent
            .get(agent_id)
            .map(|h| h.available)
            .unwrap_or(false)
    }

    /// Agents whose health-check result reported `available == true`. Sorted
    /// for deterministic snapshot output.
    pub fn healthy_agents(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self
            .preflight
            .by_agent
            .iter()
            .filter_map(|(id, h)| if h.available { Some(id.as_str()) } else { None })
            .collect();
        ids.sort();
        ids
    }

    /// Validation gate for the current step. `Some(reason)` blocks
    /// `try_advance` and surfaces in the wizard footer.
    pub fn validation_error(&self) -> Option<&'static str> {
        match self.mode {
            TeamWizardMode::Home => None,
            TeamWizardMode::Compose => self.validate_compose_step(),
            TeamWizardMode::Launch => self.validate_launch_step(),
            TeamWizardMode::Manage => None,
        }
    }

    pub fn try_advance(&mut self) -> bool {
        if self.validation_error().is_some() {
            return false;
        }
        match self.mode {
            TeamWizardMode::Home => false,
            TeamWizardMode::Compose => match self.compose_step.next() {
                Some(next) => {
                    self.compose_step = next;
                    true
                }
                None => false,
            },
            TeamWizardMode::Launch => match self.launch_step.next() {
                Some(next) => {
                    self.launch_step = next;
                    if self.launch_step == LaunchStep::PlanPreview {
                        self.build_plan_preview();
                    }
                    true
                }
                None => false,
            },
            TeamWizardMode::Manage => match self.manage_step.next() {
                Some(next) => {
                    self.manage_step = next;
                    true
                }
                None => false,
            },
        }
    }

    pub fn retreat(&mut self) -> bool {
        match self.mode {
            TeamWizardMode::Home => false,
            TeamWizardMode::Compose => match self.compose_step.previous() {
                Some(prev) => {
                    self.compose_step = prev;
                    true
                }
                None => false,
            },
            TeamWizardMode::Launch => match self.launch_step.previous() {
                Some(prev) => {
                    self.launch_step = prev;
                    true
                }
                None => false,
            },
            TeamWizardMode::Manage => match self.manage_step.previous() {
                Some(prev) => {
                    self.manage_step = prev;
                    true
                }
                None => false,
            },
        }
    }

    pub fn switch_mode(&mut self, mode: TeamWizardMode) {
        self.mode = mode;
        match mode {
            TeamWizardMode::Compose => {
                self.compose_step = ComposeStep::Source;
            }
            TeamWizardMode::Launch => {
                self.launch_step = starting_launch_step(&self.initial_input);
            }
            TeamWizardMode::Manage => {
                self.manage_step = ManageStep::List;
            }
            TeamWizardMode::Home => {}
        }
    }

    /// Pre-fill Compose with an existing preset's values so the user can
    /// edit it in place. Save writes back to the same file when the name
    /// is unchanged. Issue body called this "Extends(self)" but that
    /// surprised users — true edit semantics match the [e] label.
    pub fn jump_to_edit(&mut self, parent_name: &str) {
        let Some(team) = self.resolved_teams.get(parent_name).cloned() else {
            return;
        };
        let mut bindings: std::collections::HashMap<crate::orchestration::types::TeamRole, String> =
            std::collections::HashMap::new();
        for (role, binding) in &team.bindings {
            bindings.insert(*role, binding.agent.clone());
        }
        let tier = match team.source_tier {
            crate::orchestration::team::SourceTier::Project => ComposeTier::Project,
            _ => ComposeTier::User,
        };
        self.compose = ComposePayload {
            source: Some(ComposeSource::Blank),
            primitive: Some(team.primitive),
            bindings,
            name: team.name,
            tier,
            ..ComposePayload::default()
        };
        self.compose_step = ComposeStep::Primitive;
        self.mode = TeamWizardMode::Compose;
    }

    // ── Test-only setters ───────────────────────────────────────────────

    pub fn set_compose_step_for_test(&mut self, step: ComposeStep) {
        self.compose_step = step;
    }

    pub fn set_compose_primitive_for_test(
        &mut self,
        primitive: crate::orchestration::types::Primitive,
    ) {
        self.compose.primitive = Some(primitive);
    }

    pub fn set_compose_name_for_test(&mut self, name: &str) {
        self.compose.name = name.to_string();
    }

    pub fn set_launch_step_for_test(&mut self, step: LaunchStep) {
        self.launch_step = step;
    }

    pub fn set_launch_team_for_test(&mut self, name: &str) {
        self.launch.selected_team = Some(name.to_string());
    }

    pub fn set_launch_input_for_test(&mut self, kind: LaunchInputKind, single: Option<u64>) {
        self.launch.input_kind = kind;
        if let Some(n) = single {
            self.launch.manual_issues = vec![n];
        }
    }

    pub fn set_launch_manual_issues_for_test(&mut self, issues: Vec<u64>) {
        self.launch.manual_issues = issues;
    }

    pub fn build_plan_preview_for_test(&mut self) {
        self.build_plan_preview();
    }

    pub fn set_manage_step_for_test(&mut self, step: ManageStep) {
        self.manage_step = step;
    }

    pub fn set_manage_pending_delete_for_test(&mut self, name: &str) {
        self.manage.pending_delete = Some(name.to_string());
    }
}

fn starting_launch_step(_preselect: &Option<TeamLaunchInput>) -> LaunchStep {
    // Always start at TeamPicker; preselect populates the InputPicker fields,
    // but the user still confirms the team explicitly. Future flows could
    // skip ahead based on `_preselect`; keep the function for that hook.
    LaunchStep::TeamPicker
}

fn apply_initial_input(launch: &mut LaunchPayload, input: &TeamLaunchInput) {
    match input {
        TeamLaunchInput::Issue { number, title } => {
            launch.input_kind = LaunchInputKind::Issue;
            launch.manual_issue_title = title.clone();
            launch.manual_issues = vec![*number];
        }
        TeamLaunchInput::Milestone {
            number,
            seed_issues,
            ..
        } => {
            launch.input_kind = LaunchInputKind::Milestone;
            launch.primary_milestone = Some(*number);
            launch.manual_issues = seed_issues.clone();
        }
    }
}

impl Default for TeamWizardScreen {
    fn default() -> Self {
        Self::new(ProviderKind::default())
    }
}

impl KeymapProvider for TeamWizardScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Team Wizard",
            bindings: vec![
                KeyBinding {
                    key: "c / l / m",
                    description: "Compose / Launch / Manage (from Home)",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Next step / Confirm",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Previous step (or back to Home)",
                },
                KeyBinding {
                    key: "↑/↓",
                    description: "Navigate items",
                },
                KeyBinding {
                    key: "y / n",
                    description: "Confirm / cancel delete (Manage)",
                },
            ],
        }]
    }
}

impl Screen for TeamWizardScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        match self.mode {
            TeamWizardMode::Home => self.handle_home(*code),
            TeamWizardMode::Compose => self.handle_compose(*code),
            TeamWizardMode::Launch => self.handle_launch(*code),
            TeamWizardMode::Manage => self.handle_manage(*code),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }
}

impl TeamWizardScreen {
    fn handle_home(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Char('c') => {
                self.switch_mode(TeamWizardMode::Compose);
            }
            KeyCode::Char('l') => {
                self.switch_mode(TeamWizardMode::Launch);
            }
            KeyCode::Char('m') => {
                self.switch_mode(TeamWizardMode::Manage);
            }
            KeyCode::Esc => {
                return ScreenAction::Pop;
            }
            _ => {}
        }
        ScreenAction::None
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
