//! Step machines, payloads, and entry-context types for the Team Wizard.
//!
//! Three flat step enums mirror `MilestoneWizardStep`'s pattern: `ALL` slice,
//! `next()` / `previous()` / `is_first()` / `index()` / `total()` / `label()`.
//! Each flow (Compose / Launch / Manage) has its own enum so the Mode
//! umbrella can switch between them without conflating step numbering.

#![allow(dead_code)]

use crate::orchestration::team::{ResolvedTeam, SourceTier};
use crate::orchestration::types::{Primitive, TeamRole};
use crate::state::types::IssueNumber;
use std::collections::HashMap;

/// Top-level mode of the wizard. `Home` shows the mode picker (`c` / `l` /
/// `m`); the other variants drive their respective flow's step machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TeamWizardMode {
    #[default]
    Home,
    Compose,
    Launch,
    Manage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComposeStep {
    #[default]
    Source,
    Primitive,
    Roles,
    Overrides,
    Save,
    SaveSuccess,
    SaveFailed,
}

impl ComposeStep {
    pub const ALL: &'static [Self] = &[
        Self::Source,
        Self::Primitive,
        Self::Roles,
        Self::Overrides,
        Self::Save,
        Self::SaveSuccess,
        Self::SaveFailed,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Primitive => "Primitive",
            Self::Roles => "Roles",
            Self::Overrides => "Overrides",
            Self::Save => "Save",
            Self::SaveSuccess => "Saved",
            Self::SaveFailed => "Save Failed",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL
            .iter()
            .position(|s| s == self)
            .map(|i| i + 1)
            .unwrap_or(1)
    }

    pub const fn total() -> usize {
        Self::ALL.len()
    }

    pub fn next(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        Self::ALL.get(idx + 1).copied()
    }

    pub fn previous(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        if idx == 0 {
            None
        } else {
            Self::ALL.get(idx - 1).copied()
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::Source)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchStep {
    #[default]
    TeamPicker,
    InputPicker,
    PlanPreview,
    Confirm,
    Executing,
    LaunchSuccess,
    LaunchFailed,
}

impl LaunchStep {
    pub const ALL: &'static [Self] = &[
        Self::TeamPicker,
        Self::InputPicker,
        Self::PlanPreview,
        Self::Confirm,
        Self::Executing,
        Self::LaunchSuccess,
        Self::LaunchFailed,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::TeamPicker => "Team",
            Self::InputPicker => "Input",
            Self::PlanPreview => "Plan",
            Self::Confirm => "Confirm",
            Self::Executing => "Executing",
            Self::LaunchSuccess => "Launched",
            Self::LaunchFailed => "Launch Failed",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL
            .iter()
            .position(|s| s == self)
            .map(|i| i + 1)
            .unwrap_or(1)
    }

    pub const fn total() -> usize {
        Self::ALL.len()
    }

    pub fn next(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        Self::ALL.get(idx + 1).copied()
    }

    pub fn previous(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        if idx == 0 {
            None
        } else {
            Self::ALL.get(idx - 1).copied()
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::TeamPicker)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManageStep {
    #[default]
    List,
    DeleteConfirm,
    DeleteSuccess,
    DeleteFailed,
}

impl ManageStep {
    pub const ALL: &'static [Self] = &[
        Self::List,
        Self::DeleteConfirm,
        Self::DeleteSuccess,
        Self::DeleteFailed,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::List => "List",
            Self::DeleteConfirm => "Confirm Delete",
            Self::DeleteSuccess => "Deleted",
            Self::DeleteFailed => "Delete Failed",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL
            .iter()
            .position(|s| s == self)
            .map(|i| i + 1)
            .unwrap_or(1)
    }

    pub const fn total() -> usize {
        Self::ALL.len()
    }

    pub fn next(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        Self::ALL.get(idx + 1).copied()
    }

    pub fn previous(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        if idx == 0 {
            None
        } else {
            Self::ALL.get(idx - 1).copied()
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::List)
    }
}

/// Pre-selection passed when the wizard is entered from the issue browser
/// or milestone screen. Carries the data needed to skip InputPicker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamLaunchInput {
    Issue {
        number: u64,
        title: String,
    },
    /// Milestone "slice" — every Open issue in the milestone, seed-issues
    /// captured at entry time. The wizard rebuilds the plan via `Scheduler`.
    Milestone {
        number: u64,
        title: String,
        seed_issues: Vec<u64>,
    },
}

/// Whether a Compose flow is starting from a blank slate or extending an
/// existing preset (the Manage edit-jump uses `Extends`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ComposeSource {
    #[default]
    Blank,
    Extends(String),
}

#[derive(Debug, Clone, Default)]
pub struct ComposePayload {
    pub source: Option<ComposeSource>,
    pub primitive: Option<Primitive>,
    pub bindings: HashMap<TeamRole, String>,
    pub overrides_note: String,
    pub name: String,
    pub tier: ComposeTier,
    pub primitive_focus: usize,
    pub role_focus: usize,
    pub agent_focus: usize,
    pub source_focus: usize,
}

/// Where the Compose Save step writes the new preset. Mirrors `SourceTier`
/// but excludes BuiltIn (cannot write to embedded built-ins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComposeTier {
    #[default]
    User,
    Project,
}

impl ComposeTier {
    pub const fn as_source_tier(self) -> SourceTier {
        match self {
            Self::User => SourceTier::User,
            Self::Project => SourceTier::Project,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchInputKind {
    #[default]
    Issue,
    IssueSet,
    Milestone,
    IdeaInbox,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchPayload {
    pub selected_team: Option<String>,
    pub input_kind: LaunchInputKind,
    pub manual_issue_title: String,
    pub manual_issues: Vec<u64>,
    pub primary_milestone: Option<u64>,
    pub plan: Option<PlanPreview>,
    pub preflight: Option<Result<(), PreflightSummary>>,
    pub max_parallel: usize,
    pub team_focus: usize,
    pub input_focus: usize,
}

impl LaunchPayload {
    /// First issue when `input_kind == Issue`; `None` for set/milestone/idea.
    pub fn manual_issue(&self) -> Option<u64> {
        match self.input_kind {
            LaunchInputKind::Issue => self.manual_issues.first().copied(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanPreview {
    pub team_name: String,
    pub primitive: Primitive,
    pub levels: Vec<Vec<u64>>,
    pub auto_added: Vec<u64>,
    pub original_count: usize,
    pub final_count: usize,
    pub estimated_cost_usd: f64,
    pub max_parallel: usize,
}

/// Aggregate of pre-flight failures rendered inline on PlanPreview. Carries
/// both blocking failures (Confirm disabled) and informational warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreflightSummary {
    pub blocking: Vec<PreflightBlock>,
    pub warnings: Vec<String>,
}

impl PreflightSummary {
    pub fn is_clean(&self) -> bool {
        self.blocking.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightBlock {
    MissingRoleBinding { role: TeamRole },
    AgentUnhealthy { agent_id: String, message: String },
    MissingClaudeInMinAgents,
    OpenExternalDep { issue: u64, ext_dep: u64 },
}

impl PreflightBlock {
    pub fn render_line(&self) -> String {
        match self {
            Self::MissingRoleBinding { role } => {
                format!("Missing binding for role: {}", role_label(*role))
            }
            Self::AgentUnhealthy { agent_id, message } => {
                format!("Agent {agent_id} unhealthy: {message}")
            }
            Self::MissingClaudeInMinAgents => {
                "claude must be in min_agents (L2 orchestrator)".to_string()
            }
            Self::OpenExternalDep { issue, ext_dep } => {
                format!("Issue #{issue} blocked by open external #{ext_dep}")
            }
        }
    }
}

pub fn role_label(role: TeamRole) -> &'static str {
    match role {
        TeamRole::Implementer => "implementer",
        TeamRole::Reviewer => "reviewer",
        TeamRole::Docs => "docs",
        TeamRole::Devops => "devops",
        TeamRole::Orchestrator => "orchestrator",
        TeamRole::Triager => "triager",
        TeamRole::Researcher => "researcher",
    }
}

#[derive(Debug, Clone, Default)]
pub struct ManageState {
    pub selected_index: usize,
    pub pending_delete: Option<String>,
    pub last_error: Option<String>,
}

/// Health-check cache populated once per wizard session. `fetched == true`
/// after the first apply; subsequent apply calls REPLACE rather than merge.
/// Backed by a `BTreeMap` so iteration order is deterministic — preflight
/// summaries that include unhealthy agents render in the same order across
/// runs, keeping snapshot tests stable.
#[derive(Debug, Clone, Default)]
pub struct PreflightCache {
    pub by_agent: std::collections::BTreeMap<String, AgentHealth>,
    pub fetched: bool,
}

#[derive(Debug, Clone)]
pub struct AgentHealth {
    pub available: bool,
    pub version: Option<String>,
    pub message: String,
}

/// Build a vector of `ResolvedTeam`s filtered to a single source tier.
/// Manage uses this to show only `User` presets (built-ins and project-tier
/// teams are read-only).
pub fn filter_by_tier<'a>(
    teams: &'a HashMap<String, ResolvedTeam>,
    tier: SourceTier,
) -> Vec<&'a ResolvedTeam> {
    let mut out: Vec<&'a ResolvedTeam> = teams.values().filter(|t| t.source_tier == tier).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// IssueMeta lookup convenience for the Launch flow.
pub type IssueMetaMap = HashMap<IssueNumber, crate::orchestration::dag::IssueMeta>;

#[cfg(test)]
mod tests {
    use super::*;

    // ── ComposeStep ──────────────────────────────────────────────────────

    #[test]
    fn compose_step_total_is_seven() {
        assert_eq!(ComposeStep::total(), 7);
    }

    #[test]
    fn compose_step_source_is_index_one() {
        assert_eq!(ComposeStep::Source.index(), 1);
    }

    #[test]
    fn compose_step_save_failed_is_index_seven() {
        assert_eq!(ComposeStep::SaveFailed.index(), 7);
    }

    #[test]
    fn compose_step_source_is_first() {
        assert!(ComposeStep::Source.is_first());
    }

    #[test]
    fn compose_step_save_success_is_not_first() {
        assert!(!ComposeStep::SaveSuccess.is_first());
    }

    #[test]
    fn compose_step_source_next_is_primitive() {
        assert_eq!(ComposeStep::Source.next(), Some(ComposeStep::Primitive));
    }

    #[test]
    fn compose_step_save_failed_next_is_none() {
        assert_eq!(ComposeStep::SaveFailed.next(), None);
    }

    #[test]
    fn compose_step_source_previous_is_none() {
        assert_eq!(ComposeStep::Source.previous(), None);
    }

    #[test]
    fn compose_step_save_previous_is_overrides() {
        assert_eq!(ComposeStep::Save.previous(), Some(ComposeStep::Overrides));
    }

    #[test]
    fn compose_step_all_labels_are_nonempty() {
        for s in ComposeStep::ALL {
            assert!(!s.label().is_empty(), "empty label for {s:?}");
        }
    }

    // ── LaunchStep ───────────────────────────────────────────────────────

    #[test]
    fn launch_step_total_is_seven() {
        assert_eq!(LaunchStep::total(), 7);
    }

    #[test]
    fn launch_step_team_picker_is_first() {
        assert!(LaunchStep::TeamPicker.is_first());
    }

    #[test]
    fn launch_step_team_picker_next_is_input_picker() {
        assert_eq!(LaunchStep::TeamPicker.next(), Some(LaunchStep::InputPicker));
    }

    #[test]
    fn launch_step_launch_failed_next_is_none() {
        assert_eq!(LaunchStep::LaunchFailed.next(), None);
    }

    #[test]
    fn launch_step_all_labels_are_nonempty() {
        for s in LaunchStep::ALL {
            assert!(!s.label().is_empty(), "empty label for {s:?}");
        }
    }

    // ── ManageStep ───────────────────────────────────────────────────────

    #[test]
    fn manage_step_total_is_four() {
        assert_eq!(ManageStep::total(), 4);
    }

    #[test]
    fn manage_step_list_is_first() {
        assert!(ManageStep::List.is_first());
    }

    #[test]
    fn manage_step_list_next_is_delete_confirm() {
        assert_eq!(ManageStep::List.next(), Some(ManageStep::DeleteConfirm));
    }

    #[test]
    fn manage_step_delete_failed_next_is_none() {
        assert_eq!(ManageStep::DeleteFailed.next(), None);
    }

    #[test]
    fn manage_step_all_labels_nonempty() {
        for s in ManageStep::ALL {
            assert!(!s.label().is_empty(), "empty label for {s:?}");
        }
    }

    // ── PreflightSummary ─────────────────────────────────────────────────

    #[test]
    fn preflight_summary_is_clean_when_blocking_empty() {
        let s = PreflightSummary::default();
        assert!(s.is_clean());
    }

    #[test]
    fn preflight_summary_not_clean_with_blocking() {
        let s = PreflightSummary {
            blocking: vec![PreflightBlock::MissingClaudeInMinAgents],
            warnings: vec![],
        };
        assert!(!s.is_clean());
    }

    #[test]
    fn preflight_block_render_line_includes_role_for_missing() {
        let line = PreflightBlock::MissingRoleBinding {
            role: TeamRole::Reviewer,
        }
        .render_line();
        assert!(line.contains("reviewer"));
    }
}
