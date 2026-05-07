//! Inline unit tests for TeamWizardScreen.

use super::test_helpers::{make_health_check, make_issue_meta, make_test_team};
use super::types::{
    ComposeSource, ComposeStep, LaunchInputKind, LaunchStep, ManageStep, TeamLaunchInput,
    TeamWizardMode,
};
use super::{Screen, ScreenAction, TeamWizardScreen};
use crate::orchestration::dag::IssueState;
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::provider::types::ProviderKind;
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::KeyCode;

fn fresh() -> TeamWizardScreen {
    TeamWizardScreen::new(ProviderKind::default())
}

// ── Constructor & initial state ─────────────────────────────────────────

#[test]
fn team_wizard_new_starts_at_home_mode() {
    let s = fresh();
    assert_eq!(s.mode(), TeamWizardMode::Home);
}

#[test]
fn team_wizard_new_compose_step_is_source() {
    let s = fresh();
    assert_eq!(s.compose_step(), ComposeStep::Source);
}

#[test]
fn team_wizard_new_launch_step_is_team_picker() {
    let s = fresh();
    assert_eq!(s.launch_step(), LaunchStep::TeamPicker);
}

#[test]
fn team_wizard_new_manage_step_is_list() {
    let s = fresh();
    assert_eq!(s.manage_step(), ManageStep::List);
}

// ── with_entry ──────────────────────────────────────────────────────────

#[test]
fn with_entry_none_preselect_stays_at_home() {
    let s = TeamWizardScreen::with_entry(ProviderKind::default(), TeamWizardMode::Home, None);
    assert_eq!(s.mode(), TeamWizardMode::Home);
}

#[test]
fn with_entry_issue_preselect_mode_is_launch() {
    let s = TeamWizardScreen::with_entry(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        Some(TeamLaunchInput::Issue {
            number: 42,
            title: "feat: login".into(),
        }),
    );
    assert_eq!(s.mode(), TeamWizardMode::Launch);
    assert_eq!(s.launch_payload().input_kind, LaunchInputKind::Issue);
    assert_eq!(s.launch_payload().manual_issue(), Some(42));
}

#[test]
fn with_entry_milestone_preselect_carries_seed_issues() {
    let s = TeamWizardScreen::with_entry(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        Some(TeamLaunchInput::Milestone {
            number: 7,
            title: "v0.26.0".into(),
            seed_issues: vec![10, 11, 12],
        }),
    );
    assert_eq!(s.launch_payload().input_kind, LaunchInputKind::Milestone);
    assert_eq!(s.launch_payload().primary_milestone, Some(7));
    assert_eq!(s.launch_payload().manual_issues, vec![10, 11, 12]);
}

// ── apply_resolved_teams idempotence ────────────────────────────────────

#[test]
fn apply_resolved_teams_replaces_not_appends() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![make_test_team(
        "alpha",
        Primitive::SinglePass,
        &[],
        SourceTier::User,
    )]);
    s.apply_resolved_teams(vec![make_test_team(
        "beta",
        Primitive::SinglePass,
        &[],
        SourceTier::User,
    )]);
    let teams = s.resolved_teams();
    assert_eq!(teams.len(), 1);
    assert!(teams.contains_key("beta"));
}

#[test]
fn apply_resolved_teams_empty_clears() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![make_test_team(
        "alpha",
        Primitive::SinglePass,
        &[],
        SourceTier::User,
    )]);
    s.apply_resolved_teams(Vec::new());
    assert!(s.resolved_teams().is_empty());
}

// ── apply_health_check idempotence + is_healthy ─────────────────────────

#[test]
fn apply_health_check_replaces_not_appends() {
    let mut s = fresh();
    s.apply_health_check(vec![make_health_check("claude", true)]);
    s.apply_health_check(vec![make_health_check("claude", false)]);
    assert!(!s.is_healthy("claude"));
}

#[test]
fn is_healthy_returns_true_for_available_agent() {
    let mut s = fresh();
    s.apply_health_check(vec![make_health_check("claude", true)]);
    assert!(s.is_healthy("claude"));
}

#[test]
fn is_healthy_returns_false_for_unavailable_agent() {
    let mut s = fresh();
    s.apply_health_check(vec![make_health_check("codex", false)]);
    assert!(!s.is_healthy("codex"));
}

#[test]
fn is_healthy_returns_false_for_unknown_agent() {
    let mut s = fresh();
    s.apply_health_check(vec![make_health_check("claude", true)]);
    assert!(!s.is_healthy("ghost"));
}

#[test]
fn is_healthy_returns_false_when_cache_empty() {
    let s = fresh();
    assert!(!s.is_healthy("claude"));
}

// ── Compose validation ──────────────────────────────────────────────────

#[test]
fn compose_validation_source_step_requires_source_to_be_set() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    assert!(s.validation_error().is_some());
}

#[test]
fn compose_validation_source_step_passes_when_source_set() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose.source = Some(ComposeSource::Blank);
    assert_eq!(s.validation_error(), None);
}

#[test]
fn compose_validation_save_rejects_empty_name() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose_step = ComposeStep::Save;
    s.compose.name = String::new();
    assert!(s.validation_error().is_some());
}

#[test]
fn compose_validation_save_rejects_slash_in_name() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose_step = ComposeStep::Save;
    s.compose.name = "my/team".into();
    assert!(s.validation_error().is_some());
}

#[test]
fn compose_validation_save_rejects_leading_dot() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose_step = ComposeStep::Save;
    s.compose.name = ".hidden".into();
    assert!(s.validation_error().is_some());
}

#[test]
fn compose_validation_save_accepts_valid_name() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose_step = ComposeStep::Save;
    s.compose.name = "my-coder-v2".into();
    assert_eq!(s.validation_error(), None);
}

// ── try_advance gating ──────────────────────────────────────────────────

#[test]
fn try_advance_blocked_when_validation_error_is_some() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    let advanced = s.try_advance();
    assert!(!advanced);
    assert_eq!(s.compose_step(), ComposeStep::Source);
}

#[test]
fn try_advance_succeeds_when_validation_passes() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose.source = Some(ComposeSource::Blank);
    let advanced = s.try_advance();
    assert!(advanced);
    assert_eq!(s.compose_step(), ComposeStep::Primitive);
}

// ── Mode switching from Home ────────────────────────────────────────────

#[test]
fn home_c_key_switches_to_compose_mode() {
    let mut s = fresh();
    s.handle_input(&key_event(KeyCode::Char('c')), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Compose);
}

#[test]
fn home_l_key_switches_to_launch_mode() {
    let mut s = fresh();
    s.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Launch);
}

#[test]
fn home_m_key_switches_to_manage_mode() {
    let mut s = fresh();
    s.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Manage);
}

#[test]
fn esc_from_compose_first_step_returns_to_home() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Home);
}

#[test]
fn esc_from_launch_first_step_returns_to_home() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Home);
}

#[test]
fn esc_from_manage_first_step_returns_to_home() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Manage);
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Home);
}

#[test]
fn esc_from_home_pops_screen() {
    let mut s = fresh();
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

// ── Manage filter ───────────────────────────────────────────────────────

#[test]
fn manage_list_shows_only_user_tier_teams() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![
        make_test_team("builtin", Primitive::SinglePass, &[], SourceTier::BuiltIn),
        make_test_team("user-custom", Primitive::SinglePass, &[], SourceTier::User),
        make_test_team(
            "project-team",
            Primitive::SinglePass,
            &[],
            SourceTier::Project,
        ),
    ]);
    let result = s.manage_list_teams();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "user-custom");
}

#[test]
fn manage_list_empty_when_no_user_teams() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![
        make_test_team("a", Primitive::SinglePass, &[], SourceTier::BuiltIn),
        make_test_team("b", Primitive::SinglePass, &[], SourceTier::Project),
    ]);
    assert!(s.manage_list_teams().is_empty());
}

// ── Manage edit-jump ────────────────────────────────────────────────────

#[test]
fn manage_e_key_jumps_to_compose_with_extends_source() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![make_test_team(
        "base-team",
        Primitive::SinglePass,
        &[],
        SourceTier::User,
    )]);
    s.switch_mode(TeamWizardMode::Manage);
    s.handle_input(&key_event(KeyCode::Char('e')), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Compose);
    assert_eq!(
        s.compose_payload().source,
        Some(ComposeSource::Extends("base-team".into()))
    );
    assert_eq!(s.compose_step(), ComposeStep::Primitive);
}

// ── Launch plan-preview build ───────────────────────────────────────────

fn pipeline_team() -> crate::orchestration::team::ResolvedTeam {
    make_test_team(
        "default-coder",
        Primitive::Pipeline,
        &[
            (TeamRole::Implementer, "claude"),
            (TeamRole::Reviewer, "claude"),
            (TeamRole::Docs, "claude"),
        ],
        SourceTier::BuiltIn,
    )
}

#[test]
fn launch_plan_preview_original_count_matches_manual_selection() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.launch.selected_team = Some("default-coder".into());
    s.launch.input_kind = LaunchInputKind::IssueSet;
    s.launch.manual_issues = vec![1, 2, 3];
    s.launch.primary_milestone = Some(1);
    let mut metas = std::collections::HashMap::new();
    metas.insert(1, make_issue_meta(1, IssueState::Open, Some(1), &[]));
    metas.insert(2, make_issue_meta(2, IssueState::Open, Some(1), &[]));
    metas.insert(3, make_issue_meta(3, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.build_plan_preview();
    let plan = s.launch_payload().plan.as_ref().expect("plan built");
    assert_eq!(plan.original_count, 3);
    assert_eq!(plan.final_count, 3);
}

#[test]
fn launch_plan_preview_cost_estimate_is_positive_for_claude_pipeline() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.launch.selected_team = Some("default-coder".into());
    s.launch.input_kind = LaunchInputKind::Issue;
    s.launch.manual_issues = vec![1];
    let mut metas = std::collections::HashMap::new();
    metas.insert(1, make_issue_meta(1, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.build_plan_preview();
    let plan = s.launch_payload().plan.as_ref().expect("plan built");
    assert!(
        plan.estimated_cost_usd > 0.0,
        "expected positive cost, got {}",
        plan.estimated_cost_usd
    );
}

// ── Launch preflight gating ─────────────────────────────────────────────

#[test]
fn launch_confirm_enter_is_noop_when_preflight_blocking() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.launch_step = LaunchStep::PlanPreview;
    s.launch.preflight = Some(Err(super::types::PreflightSummary {
        blocking: vec![super::types::PreflightBlock::MissingClaudeInMinAgents],
        warnings: vec![],
    }));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::PlanPreview);
}

#[test]
fn launch_confirm_advances_to_confirm_when_preflight_ok() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.launch_step = LaunchStep::PlanPreview;
    s.launch.preflight = Some(Ok(()));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::Confirm);
}

// ── apply_save_result / apply_launch_result / apply_delete_result ───────

#[test]
fn apply_save_result_ok_transitions_to_save_success() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose_step = ComposeStep::Save;
    s.apply_save_result(Ok(()));
    assert_eq!(s.compose_step(), ComposeStep::SaveSuccess);
}

#[test]
fn apply_save_result_err_transitions_to_save_failed() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.compose_step = ComposeStep::Save;
    s.apply_save_result(Err("disk full".into()));
    assert_eq!(s.compose_step(), ComposeStep::SaveFailed);
    assert_eq!(s.failure_reason(), Some("disk full"));
}

#[test]
fn apply_launch_result_ok_transitions_to_launch_success() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.launch_step = LaunchStep::Executing;
    s.apply_launch_result(Ok(()));
    assert_eq!(s.launch_step(), LaunchStep::LaunchSuccess);
}

#[test]
fn apply_delete_result_err_transitions_to_delete_failed() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Manage);
    s.manage.pending_delete = Some("user-team".into());
    s.apply_delete_result(Err("permission denied".into()));
    assert_eq!(s.manage_step(), ManageStep::DeleteFailed);
}
