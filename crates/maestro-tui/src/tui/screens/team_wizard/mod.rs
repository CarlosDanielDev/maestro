//! Team Wizard.
//!
//! Home is the Teams overview: every preset by tier with readiness and the
//! selected preset's card (#1286). Three flows start from it:
//! - Compose: Start from → Shape → Roles → Review & save (armed write)
//! - Launch:  Team → Input → Plan & launch, then the launch dialog (#1290)
//! - Manage:  list user-tier presets, edit (jumps into Compose), delete
//!
//! Mirrors the `milestone_wizard` step-machine pattern.

#![allow(dead_code)]

pub mod clipboard;
mod compose;
mod compose_save;
mod draw;
mod draw_compose;
mod draw_launch;
mod draw_launch_plan;
mod issue_paste;
mod launch;
mod launch_cost;
mod launch_plan;
mod launch_waves;
mod manage;
mod overview;
pub mod types;

#[cfg(test)]
pub mod test_helpers;

pub use types::{
    AgentHealth, ComposePayload, ComposeSource, ComposeStep, ComposeTier, LaunchInputKind,
    LaunchPayload, LaunchPhase, LaunchStep, ManageState, ManageStep, PreflightCache,
    TeamLaunchInput, TeamWizardMode, roles_to_bind,
};

use crate::orchestration::dag::IssueMeta;
use crate::orchestration::team::ResolvedTeam;
use crate::provider::types::ProviderKind;
use crate::state::types::IssueNumber;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crate::tui::widgets::input::{Input, InputConfig};
use crate::tui::widgets::launch_dialog::LaunchBudget;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use super::{Screen, ScreenAction};

/// Top-level wizard screen. Owns step machines for all three flows plus
/// session-scoped caches (resolved teams, health check, issue metas).
pub struct TeamWizardScreen {
    provider_kind: ProviderKind,
    mode: TeamWizardMode,
    compose_step: ComposeStep,
    launch_step: LaunchStep,
    launch_phase: LaunchPhase,
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
    /// True when the user entered Compose via Manage `[e]`. Save-success
    /// returns to the Manage list instead of popping the wizard, preserving
    /// the navigation context the user came from.
    pub(super) editing_existing: bool,
    /// Clipboard read seam — owned, never shared. Defaults to the system
    /// backend; tests inject `StubClipboard` via `with_clipboard_for_test`.
    pub(super) clipboard: Box<dyn clipboard::IssueClipboard>,
    /// Review & save name field; `compose.name` mirrors its text.
    pub(super) name_input: Input,
    /// User-tier presets dir and the repo root of the project tier, read
    /// once; tests point them at a tempdir.
    pub(super) user_dir: Option<PathBuf>,
    pub(super) project_root: Option<PathBuf>,
    /// Tests inject the clock for the armed write; the app uses `now`.
    pub(super) clock: Option<Instant>,
    /// Selected row in the Home overview (`overview::ordered_presets`).
    pub(super) home_focus: usize,
    /// Launch Input: the find-issue field (#1290).
    pub(super) issue_input: Input,
    /// The money the Plan's cost line states, fed each frame from the
    /// launch dialog's source (`budget_prespawn::launch_budget`).
    pub(super) launch_budget: LaunchBudget,
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
        let issue_text = match &preselect {
            Some(TeamLaunchInput::Issue { number, .. }) => number.to_string(),
            _ => String::new(),
        };
        if let Some(input) = preselect {
            apply_initial_input(&mut launch, &input);
        }
        Self {
            provider_kind,
            mode,
            compose_step: ComposeStep::default(),
            launch_step: LaunchStep::Team,
            launch_phase: LaunchPhase::Editing,
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
            editing_existing: false,
            clipboard: Box::new(clipboard::SystemIssueClipboard),
            name_input: new_name_input(""),
            // Tests never get the real config dir or cwd: a write without
            // `set_preset_dirs_for_test` fails instead of touching them.
            user_dir: if cfg!(test) {
                None
            } else {
                crate::orchestration::loader::Loader::user_tier_default()
            },
            project_root: if cfg!(test) {
                None
            } else {
                std::env::current_dir().ok()
            },
            clock: None,
            home_focus: 0,
            issue_input: new_issue_input(&issue_text),
            launch_budget: LaunchBudget::default(),
        }
    }

    #[cfg(test)]
    pub fn with_clipboard_for_test(
        provider_kind: ProviderKind,
        mode: TeamWizardMode,
        preselect: Option<TeamLaunchInput>,
        clipboard: Box<dyn clipboard::IssueClipboard>,
    ) -> Self {
        let mut s = Self::with_entry(provider_kind, mode, preselect);
        s.clipboard = clipboard;
        s
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

    pub fn launch_phase(&self) -> LaunchPhase {
        self.launch_phase
    }

    /// The spend the Plan states, set before every draw (#1290).
    pub fn set_launch_budget(&mut self, budget: LaunchBudget) {
        self.launch_budget = budget;
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

    /// The health-check cache (drives preset readiness and the card).
    pub fn preflight_cache(&self) -> &PreflightCache {
        &self.preflight
    }

    /// True when the file Review & save would write already exists: the
    /// real path on disk, not the merged preset list (which can be empty
    /// after a failed load, or hold another tier's preset of that name).
    pub fn compose_target_exists(&self) -> bool {
        self.compose_preview().is_ok_and(|(path, _)| path.exists())
    }

    /// Name of the preset selected on the Home overview.
    pub fn selected_preset(&self) -> Option<&str> {
        overview::ordered_presets(&self.resolved_teams)
            .get(self.home_focus)
            .map(|t| t.name.as_str())
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

    pub fn is_editing_existing(&self) -> bool {
        self.editing_existing
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
        self.home_focus = self
            .home_focus
            .min(self.resolved_teams.len().saturating_sub(1));
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
                    checked: r.message != types::SEEDED_HEALTH_MESSAGE,
                    version: r.version,
                    message: r.message,
                },
            );
        }
        self.preflight.fetched = true;
    }

    /// Replace the issue-meta cache used by Launch's plan preview.
    /// A plan on screen (Plan or its dialog) is rebuilt from the new data,
    /// and an open dialog goes back to the plan, so what dispatches is what
    /// was shown (#1290 judge).
    pub fn apply_issue_metas(&mut self, metas: HashMap<IssueNumber, IssueMeta>) {
        self.issue_metas = metas;
        let on_plan = self.mode == TeamWizardMode::Launch
            && self.launch_step == LaunchStep::Plan
            && matches!(
                self.launch_phase,
                LaunchPhase::Editing | LaunchPhase::Dialog
            );
        if on_plan && self.launch.plan.is_some() {
            self.build_plan_preview();
            self.launch_phase = LaunchPhase::Editing;
        }
    }

    /// Read-only borrow of the issue-meta cache. Used by the dispatcher when
    /// rebuilding a `Scheduler` for a LaunchTeam fan-out (#877).
    pub fn issue_metas(&self) -> &HashMap<IssueNumber, IssueMeta> {
        &self.issue_metas
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
            TeamWizardMode::Compose => {
                match compose::next_compose_step(self.compose_step, self.compose_has_roles()) {
                    Some(next) => {
                        self.set_compose_step(next);
                        true
                    }
                    None => false,
                }
            }
            TeamWizardMode::Launch => match self.launch_step.next() {
                Some(next) => {
                    self.launch_step = next;
                    if self.launch_step == LaunchStep::Plan {
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
            TeamWizardMode::Compose => {
                match compose::previous_compose_step(self.compose_step, self.compose_has_roles()) {
                    Some(prev) => {
                        self.set_compose_step(prev);
                        true
                    }
                    None => false,
                }
            }
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
                self.set_compose_step(ComposeStep::Source);
            }
            TeamWizardMode::Launch => {
                self.launch_step = LaunchStep::Team;
                self.launch_phase = LaunchPhase::Editing;
            }
            TeamWizardMode::Manage => {
                self.manage_step = ManageStep::List;
            }
            TeamWizardMode::Home => {}
        }
    }

    /// Pre-fill Compose with an existing preset's values so the user can
    /// edit it in place. Save writes back to the same file when the name
    /// is unchanged.
    pub fn jump_to_edit(&mut self, parent_name: &str) {
        let Some(team) = self.resolved_teams.get(parent_name) else {
            return;
        };
        let bindings = team
            .bindings
            .iter()
            .map(|(role, b)| (*role, b.agent.clone()))
            .collect();
        // Manage filters User-tier only, so BuiltIn shouldn't reach here;
        // collapse defensively. Project keeps its own tier so re-saves
        // overwrite the project file.
        let tier = match team.source_tier {
            crate::orchestration::team::SourceTier::Project => ComposeTier::Project,
            crate::orchestration::team::SourceTier::User
            | crate::orchestration::team::SourceTier::BuiltIn => ComposeTier::User,
        };
        let primitive = team.primitive;
        let name = team.name.clone();
        self.name_input = new_name_input(&name);
        self.compose = ComposePayload {
            source: Some(ComposeSource::Blank),
            primitive: Some(primitive),
            bindings,
            name,
            tier,
            ..ComposePayload::default()
        };
        self.editing_existing = true;
        self.set_compose_step(ComposeStep::Primitive);
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

    pub fn set_compose_source_for_test(&mut self, source: ComposeSource) {
        self.compose.source = Some(source);
    }

    pub fn set_compose_binding_for_test(
        &mut self,
        role: crate::orchestration::types::TeamRole,
        agent: &str,
    ) {
        self.compose.bindings.insert(role, agent.to_string());
    }

    pub fn set_compose_name_for_test(&mut self, name: &str) {
        self.compose.name = name.to_string();
        self.name_input = new_name_input(name);
    }

    pub fn set_preset_dirs_for_test(&mut self, user_dir: PathBuf, project_root: PathBuf) {
        self.user_dir = Some(user_dir);
        self.project_root = Some(project_root);
    }

    pub fn set_clock_for_test(&mut self, now: Instant) {
        self.clock = Some(now);
    }

    /// Put Review & save in the state a finished write leaves it in.
    pub fn set_compose_save_result_for_test(&mut self, result: Result<(), String>) {
        self.set_compose_step(ComposeStep::Save);
        let now = self.now();
        self.compose.save.on_enter(now);
        self.compose.save.on_enter(now);
        self.compose.save.finish(result);
    }

    pub fn set_launch_step_for_test(&mut self, step: LaunchStep) {
        self.launch_step = step;
    }

    pub fn set_launch_phase_for_test(&mut self, phase: LaunchPhase) {
        self.launch_phase = phase;
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

    /// Focus the Input step's find field with `input` typed in it.
    pub fn set_launch_manual_issue_input_for_test(&mut self, input: &str) {
        self.launch_step = LaunchStep::Input;
        self.launch.finding = true;
        self.issue_input.set_text(input);
    }

    pub fn set_autocomplete_focus_for_test(&mut self, focus: Option<usize>) {
        self.launch.autocomplete_focus = focus;
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

fn new_name_input(initial: &str) -> Input {
    Input::single_line(
        InputConfig::new()
            .max_len(compose_save::NAME_MAX)
            .placeholder("my-team")
            .initial(initial),
    )
}

/// The Input step's find field: `#n` or a number (#1290).
fn new_issue_input(initial: &str) -> Input {
    Input::single_line(
        InputConfig::new()
            .max_len(12)
            .placeholder("issue number, like 42")
            .initial(initial),
    )
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
        // Home and Compose keys live in the registry TeamWizard block
        // (#1286); this group lists only the Launch / Manage keys it lacks.
        vec![KeyBindingGroup {
            title: "Team Wizard",
            bindings: vec![
                KeyBinding {
                    key: "e / d",
                    description: "Edit / delete a preset (Manage)",
                },
                KeyBinding {
                    key: "y / n",
                    description: "Confirm / cancel delete (Manage)",
                },
                KeyBinding {
                    key: "r",
                    description: "Retry after a failed launch or delete",
                },
            ],
        }]
    }
}

impl Screen for TeamWizardScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        match self.mode {
            TeamWizardMode::Home => self.handle_home(*code),
            TeamWizardMode::Compose => self.handle_compose(event, *code),
            TeamWizardMode::Launch => self.handle_launch(event, *code, *modifiers),
            TeamWizardMode::Manage => self.handle_manage(*code),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let now = self.now();
        self.compose.save.tick(now);
        self.draw_impl(f, area, theme);
    }

    /// The Review & save name field and Launch's find-issue field are text
    /// inputs: letters type there, never reach global keys.
    fn desired_input_mode(&self) -> Option<InputMode> {
        let naming = self.mode == TeamWizardMode::Compose
            && self.compose_step == ComposeStep::Save
            && *self.compose.save.state() != crate::tui::ds::armed::ArmedState::Done;
        let finding = self.mode == TeamWizardMode::Launch
            && self.launch_phase == LaunchPhase::Editing
            && self.launch_step == LaunchStep::Input
            && self.launch.finding;
        let typing = naming || finding;
        Some(if typing {
            InputMode::Insert
        } else {
            InputMode::Normal
        })
    }

    /// Home, Compose and Launch put their keys in the shell footer. Manage
    /// draws `WizardFrame::draw`'s own step hints; the shell footer
    /// keeps to the two keys every step has.
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.mode {
            TeamWizardMode::Home => overview::footer_hints(),
            TeamWizardMode::Compose => draw_compose::footer_hints(self),
            TeamWizardMode::Launch => draw_launch::footer_hints(self),
            TeamWizardMode::Manage => {
                vec![("Enter", "next"), ("Esc", "back")]
            }
        }
    }
}

impl TeamWizardScreen {
    fn handle_home(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.home_focus = self.home_focus.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                let last = overview::ordered_presets(&self.resolved_teams)
                    .len()
                    .saturating_sub(1);
                self.home_focus = (self.home_focus + 1).min(last);
            }
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

#[cfg(test)]
#[path = "cancel_tests.rs"]
mod cancel_tests;

#[cfg(test)]
#[path = "compose_tests.rs"]
mod compose_tests;

#[cfg(test)]
#[path = "review_tests.rs"]
mod review_tests;

#[cfg(test)]
#[path = "race_tests.rs"]
mod race_tests;

#[cfg(test)]
#[path = "launch_tests.rs"]
mod launch_tests;

#[cfg(test)]
#[path = "launch_gate_tests.rs"]
mod launch_gate_tests;
