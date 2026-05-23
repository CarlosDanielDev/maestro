//! Inline unit tests for TeamWizardScreen.

use super::test_helpers::{make_health_check, make_issue_meta, make_test_team};
use super::types::{
    ComposeSource, ComposeStep, LaunchInputKind, LaunchStep, ManageStep, TeamLaunchInput,
    TeamWizardMode,
};
use super::{Screen, ScreenAction, TeamWizardScreen};
use crate::orchestration::dag::IssueState;
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamInput, TeamRole};
use crate::provider::types::ProviderKind;
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
use crossterm::event::{KeyCode, KeyModifiers};

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
fn save_success_enter_returns_to_manage_when_editing() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![make_test_team(
        "base-team",
        Primitive::SinglePass,
        &[],
        SourceTier::User,
    )]);
    s.switch_mode(TeamWizardMode::Manage);
    s.handle_input(&key_event(KeyCode::Char('e')), InputMode::Normal);
    assert!(s.is_editing_existing());
    s.set_compose_step_for_test(ComposeStep::SaveSuccess);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.mode(), TeamWizardMode::Manage);
    assert_eq!(s.manage_step(), ManageStep::List);
    assert!(!s.is_editing_existing());
}

#[test]
fn save_success_enter_pops_when_creating_new() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_step_for_test(ComposeStep::SaveSuccess);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn manage_e_key_loads_preset_into_compose_payload() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![make_test_team(
        "base-team",
        Primitive::Pipeline,
        &[(TeamRole::Reviewer, "claude")],
        SourceTier::User,
    )]);
    s.switch_mode(TeamWizardMode::Manage);
    s.handle_input(&key_event(KeyCode::Char('e')), InputMode::Normal);
    assert_eq!(s.mode(), TeamWizardMode::Compose);
    assert_eq!(s.compose_payload().source, Some(ComposeSource::Blank));
    assert_eq!(s.compose_payload().primitive, Some(Primitive::Pipeline));
    assert_eq!(s.compose_payload().name, "base-team");
    assert_eq!(
        s.compose_payload()
            .bindings
            .get(&TeamRole::Reviewer)
            .map(String::as_str),
        Some("claude")
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

// ── Launch IssuePicker (issue #805) ─────────────────────────────────────

#[test]
fn launch_input_picker_issue_kind_enter_advances_to_issue_picker() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::InputPicker);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::IssuePicker);
}

#[test]
fn launch_issue_picker_enter_with_empty_buffer_does_not_advance() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::IssuePicker);
    assert_eq!(s.validation_error(), Some("Enter an issue number"));
}

#[test]
fn launch_issue_picker_enter_with_non_numeric_buffer_does_not_advance() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.set_launch_manual_issue_input_for_test("abc");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::IssuePicker);
    assert_eq!(s.validation_error(), Some("Enter a valid issue number"));
}

#[test]
fn launch_issue_picker_enter_with_zero_rejected() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.set_launch_manual_issue_input_for_test("0");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::IssuePicker);
    assert_eq!(
        s.validation_error(),
        Some("Issue number must be greater than 0")
    );
}

#[test]
fn launch_issue_picker_enter_with_valid_number_populates_manual_issues_and_advances() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.launch.input_kind = LaunchInputKind::Issue;
    s.handle_input(&key_event(KeyCode::Char('4')), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Char('2')), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issues, vec![42u64]);
    assert_eq!(s.launch_step(), LaunchStep::PlanPreview);
}

#[test]
fn launch_issue_picker_esc_returns_to_input_picker() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.launch.input_kind = LaunchInputKind::Issue;
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::InputPicker);
    assert_eq!(s.launch_payload().input_kind, LaunchInputKind::Issue);
}

#[test]
fn launch_issue_picker_backspace_pops_last_digit() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.set_launch_manual_issue_input_for_test("42");
    s.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issue_input, "4");
}

#[test]
fn launch_issue_picker_digit_keystroke_appends_to_buffer() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(&key_event(KeyCode::Char('4')), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Char('2')), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issue_input, "42");
}

#[test]
fn launch_issue_picker_non_digit_keystroke_ignored() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issue_input, "");
}

#[test]
fn launch_issue_picker_buffer_capped_at_ten_digits() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    for _ in 0..12 {
        s.handle_input(&key_event(KeyCode::Char('1')), InputMode::Normal);
    }
    assert_eq!(s.launch_payload().manual_issue_input.len(), 10);
}

#[test]
fn launch_input_picker_issue_set_enter_skips_issue_picker() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(vec![1, 2]);
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::InputPicker);
    s.launch.input_focus = 1;
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::PlanPreview);
}

// ── #877 — launch dispatch wiring ───────────────────────────────────────

#[test]
fn launch_dispatch_returns_launch_team_action_with_issue_input() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.launch.input_kind = LaunchInputKind::Issue;
    s.launch.manual_issues = vec![42];
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::Confirm);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(
        action,
        ScreenAction::LaunchTeam {
            team_name: "default-coder".into(),
            input: TeamInput::Issue { number: 42 },
            max_parallel: 3,
        }
    );
    assert_eq!(s.launch_step(), LaunchStep::Executing);
}

#[test]
fn launch_dispatch_returns_launch_team_action_with_idea_inbox() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.launch.input_kind = LaunchInputKind::IdeaInbox;
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::Confirm);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(
        action,
        ScreenAction::LaunchTeam {
            team_name: "default-coder".into(),
            input: TeamInput::IdeaInbox,
            max_parallel: 3,
        }
    );
}

#[test]
fn launch_dispatch_no_team_selected_returns_none() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::Confirm);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_step(), LaunchStep::Confirm);
}

#[test]
fn esc_on_executing_step_is_swallowed() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::Executing);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_step(), LaunchStep::Executing);
    assert_eq!(s.mode(), TeamWizardMode::Launch);
}

#[test]
fn esc_on_launch_success_pops_screen_does_not_retreat() {
    // Regression: pressing Esc on the LaunchSuccess "Launched" step used to
    // call `handle_launch_back` → `retreat()` which walked the user back to
    // Executing. Combined with the Esc-on-Executing swallow this stranded
    // the wizard with no forward path. Esc on terminal states must Pop.
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::LaunchSuccess);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
    assert_eq!(s.launch_step(), LaunchStep::LaunchSuccess);
}

#[test]
fn esc_on_launch_failed_pops_screen_does_not_retreat() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::LaunchFailed);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
    assert_eq!(s.launch_step(), LaunchStep::LaunchFailed);
}

// ── #875 — Ctrl+V paste behaviour ───────────────────────────────────────

#[test]
fn ctrl_v_paste_populates_issue_buffer() {
    use super::clipboard::testing::StubClipboard;
    let mut s = TeamWizardScreen::with_clipboard_for_test(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        None,
        Box::new(StubClipboard::with_text("#777")),
    );
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
        InputMode::Normal,
    );
    assert_eq!(s.launch_payload().manual_issue_input, "777");
}

#[test]
fn ctrl_v_paste_non_numeric_leaves_buffer_unchanged() {
    use super::clipboard::testing::StubClipboard;
    let mut s = TeamWizardScreen::with_clipboard_for_test(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        None,
        Box::new(StubClipboard::with_text("abc")),
    );
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
        InputMode::Normal,
    );
    assert_eq!(s.launch_payload().manual_issue_input, "");
}

#[test]
fn ctrl_v_paste_with_empty_clipboard_is_noop() {
    use super::clipboard::testing::StubClipboard;
    let mut s = TeamWizardScreen::with_clipboard_for_test(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        None,
        Box::new(StubClipboard::empty()),
    );
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.set_launch_manual_issue_input_for_test("99");
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
        InputMode::Normal,
    );
    assert_eq!(s.launch_payload().manual_issue_input, "99");
}

#[test]
fn ctrl_v_paste_ignored_outside_issue_picker_step() {
    use super::clipboard::testing::StubClipboard;
    let mut s = TeamWizardScreen::with_clipboard_for_test(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        None,
        Box::new(StubClipboard::with_text("#42")),
    );
    // Leave default step (TeamPicker)
    let before_step = s.launch_step();
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
        InputMode::Normal,
    );
    assert_eq!(s.launch_step(), before_step);
    assert_eq!(s.launch_payload().manual_issue_input, "");
}

#[test]
fn ctrl_v_paste_truncates_long_issue_number() {
    use super::clipboard::testing::StubClipboard;
    let mut s = TeamWizardScreen::with_clipboard_for_test(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        None,
        Box::new(StubClipboard::with_text("12345678901234")),
    );
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
        InputMode::Normal,
    );
    assert_eq!(s.launch_payload().manual_issue_input.len(), 10);
    assert_eq!(s.launch_payload().manual_issue_input, "1234567890");
}

// ── #876 — Autocomplete candidates + key arms ───────────────────────────

#[test]
fn autocomplete_candidates_empty_buffer_returns_empty() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    metas.insert(1u64, make_issue_meta(1, IssueState::Open, None, &[]));
    metas.insert(10u64, make_issue_meta(10, IssueState::Open, None, &[]));
    s.apply_issue_metas(metas);
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn autocomplete_candidates_filters_by_prefix() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 12, 100, 101, 200] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);

    s.set_launch_manual_issue_input_for_test("1");
    assert_eq!(s.autocomplete_candidates(), vec![1, 10, 12, 100, 101]);

    s.set_launch_manual_issue_input_for_test("10");
    assert_eq!(s.autocomplete_candidates(), vec![10, 100, 101]);

    s.set_launch_manual_issue_input_for_test("5");
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn autocomplete_candidates_excludes_closed_issues() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    metas.insert(42u64, make_issue_meta(42, IssueState::Closed, None, &[]));
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("4");
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn autocomplete_candidates_top_five_only() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 11, 12, 13, 14, 15, 100, 101, 111] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("1");
    let cands = s.autocomplete_candidates();
    assert_eq!(cands.len(), 5);
    assert_eq!(cands, vec![1, 10, 11, 12, 13]);
}

#[test]
fn autocomplete_candidates_with_no_metas_returns_empty() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.set_launch_manual_issue_input_for_test("42");
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn arrow_down_moves_autocomplete_focus() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 12] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("1");

    assert_eq!(s.launch_payload().autocomplete_focus, None);
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(0));
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(1));
}

#[test]
fn arrow_up_clamps_at_zero() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 12] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("1");
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(0));
    s.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(0));
}

#[test]
fn autocomplete_focus_stays_none_on_down_when_no_candidates() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, None);
}

#[test]
fn tab_with_focused_candidate_replaces_buffer() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 12] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("1");
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issue_input, "10");
    assert_eq!(s.launch_payload().autocomplete_focus, None);
}

#[test]
fn tab_with_no_candidates_is_noop() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issue_input, "");
    assert_eq!(s.launch_payload().autocomplete_focus, None);
}

#[test]
fn enter_ignores_autocomplete_focus() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    s.launch.input_kind = LaunchInputKind::Issue;
    let mut metas = std::collections::HashMap::new();
    metas.insert(42u64, make_issue_meta(42, IssueState::Open, Some(1), &[]));
    metas.insert(420u64, make_issue_meta(420, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("42");
    s.set_autocomplete_focus_for_test(Some(1));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_payload().manual_issues, vec![42u64]);
    assert_eq!(s.launch_step(), LaunchStep::PlanPreview);
}

#[test]
fn backspace_resets_autocomplete_focus() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 12] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("12");
    s.set_autocomplete_focus_for_test(Some(2));
    s.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, None);
    assert_eq!(s.launch_payload().manual_issue_input, "1");
}

#[test]
fn digit_keystroke_resets_autocomplete_focus() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::IssuePicker);
    let mut metas = std::collections::HashMap::new();
    for n in [1u64, 10, 12] {
        metas.insert(n, make_issue_meta(n, IssueState::Open, None, &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("1");
    s.set_autocomplete_focus_for_test(Some(0));
    s.handle_input(&key_event(KeyCode::Char('0')), InputMode::Normal);
    assert_eq!(s.launch_payload().autocomplete_focus, None);
    assert_eq!(s.launch_payload().manual_issue_input, "10");
}
