//! Inline unit tests for TeamWizardScreen.

use super::test_helpers::{make_health_check, make_issue_meta, make_test_team};
use super::types::{
    ComposeSource, ComposeStep, LaunchInputKind, LaunchPhase, LaunchStep, ManageStep,
    TeamLaunchInput, TeamWizardMode,
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
    assert_eq!(s.launch_step(), LaunchStep::Team);
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
    s.set_compose_save_result_for_test(Ok(()));
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
    s.set_compose_save_result_for_test(Ok(()));
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
fn launch_plan_preview_worst_case_is_issues_times_cap() {
    let mut s = fresh();
    s.set_launch_budget(crate::tui::widgets::launch_dialog::LaunchBudget {
        per_session_usd: 5.0,
        ..Default::default()
    });
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
    assert_eq!(plan.worst_case_usd, 5.0, "1 issue × $5.00 cap");
}

// ── Launch preflight gating ─────────────────────────────────────────────

#[test]
fn launch_plan_enter_is_noop_when_preflight_blocking() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.launch_step = LaunchStep::Plan;
    s.launch.preflight = Some(Err(super::types::PreflightSummary {
        blocking: vec![super::types::PreflightBlock::MissingClaudeInMinAgents],
        warnings: vec![],
    }));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
    assert_eq!(s.launch_phase(), LaunchPhase::Editing);
    assert_eq!(
        s.launch.error,
        Some("Pre-flight failed · fix the blockers first")
    );
}

#[test]
fn launch_plan_enter_opens_the_dialog_when_preflight_ok() {
    let mut s = planned_issue(42);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_phase(), LaunchPhase::Dialog);
}

// ── apply_save_result / apply_launch_result / apply_delete_result ───────

#[test]
fn save_result_ok_marks_the_write_bar_done() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_save_result_for_test(Ok(()));
    assert_eq!(s.compose_step(), ComposeStep::Save);
    assert_eq!(
        *s.compose_save_state(),
        crate::tui::ds::armed::ArmedState::Done
    );
}

#[test]
fn save_result_err_marks_the_write_bar_failed_with_the_reason() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_save_result_for_test(Err("disk full".into()));
    assert_eq!(s.compose_step(), ComposeStep::Save);
    assert_eq!(
        *s.compose_save_state(),
        crate::tui::ds::armed::ArmedState::Failed("disk full".into())
    );
}

#[test]
fn apply_launch_result_ok_transitions_to_launch_success() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_phase_for_test(LaunchPhase::Executing);
    s.apply_launch_result(Ok(()));
    assert_eq!(s.launch_phase(), LaunchPhase::Launched);
}

#[test]
fn apply_delete_result_err_transitions_to_delete_failed() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Manage);
    s.manage.pending_delete = Some("user-team".into());
    s.apply_delete_result(Err("permission denied".into()));
    assert_eq!(s.manage_step(), ManageStep::DeleteFailed);
}

// ── Launch Input: find issue (issue #805, #1290) ────────────────────────

/// Launch on Plan & launch for `#n`: team picked, plan built, preflight ok.
fn planned_issue(n: u64) -> TeamWizardScreen {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_input_for_test(LaunchInputKind::Issue, Some(n));
    let mut metas = std::collections::HashMap::new();
    metas.insert(n, make_issue_meta(n, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    assert_eq!(s.launch.preflight, Some(Ok(())), "fixture preflight is ok");
    s
}

/// Launch on Input with the find field focused and empty.
fn finding() -> TeamWizardScreen {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_manual_issue_input_for_test("");
    s
}

fn metas_open(
    numbers: &[u64],
) -> std::collections::HashMap<u64, crate::orchestration::dag::IssueMeta> {
    numbers
        .iter()
        .map(|n| (*n, make_issue_meta(*n, IssueState::Open, None, &[])))
        .collect()
}

fn typed(s: &mut TeamWizardScreen, text: &str) {
    for c in text.chars() {
        s.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
    }
}

#[test]
fn launch_input_issue_kind_enter_focuses_the_find_field() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::Input);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert!(s.launch.finding);
    assert_eq!(s.desired_input_mode(), Some(InputMode::Insert));
}

#[test]
fn launch_find_enter_with_empty_field_does_not_advance() {
    let mut s = finding();
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert_eq!(s.launch.error, Some("Type an issue number"));
}

#[test]
fn launch_find_enter_with_non_numeric_text_does_not_advance() {
    let mut s = finding();
    typed(&mut s, "abc");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert_eq!(s.launch.error, Some("Type an issue number, like 42 or #42"));
}

#[test]
fn launch_find_enter_with_zero_rejected() {
    let mut s = finding();
    typed(&mut s, "0");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert_eq!(s.launch.error, Some("Issue number must be greater than 0"));
}

#[test]
fn launch_find_enter_with_valid_number_populates_manual_issues_and_advances() {
    let mut s = finding();
    typed(&mut s, "#42");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_payload().manual_issues, vec![42u64]);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
    assert!(!s.launch.finding);
}

#[test]
fn launch_find_esc_returns_to_the_kind_list() {
    let mut s = finding();
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert!(!s.launch.finding);
    assert_eq!(s.launch_payload().input_kind, LaunchInputKind::Issue);
}

#[test]
fn launch_find_backspace_deletes_the_last_char() {
    let mut s = finding();
    s.set_launch_manual_issue_input_for_test("42");
    s.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
    assert_eq!(s.issue_input.text(), "4");
}

#[test]
fn launch_find_keystrokes_type_into_the_canonical_input() {
    let mut s = finding();
    typed(&mut s, "4j2");
    assert_eq!(s.issue_input.text(), "4j2", "j types, it does not move");
}

#[test]
fn launch_find_field_capped_at_twelve_chars() {
    let mut s = finding();
    typed(&mut s, "11111111111111");
    assert_eq!(s.issue_input.text().len(), 12);
}

#[test]
fn launch_input_issue_set_enter_goes_straight_to_plan() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(vec![1, 2]);
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_step_for_test(LaunchStep::Input);
    s.launch.input_focus = 1;
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
}

/// §02.17 finding 5: an empty set is a dead end; it says why.
#[test]
fn launch_input_empty_issue_set_says_why() {
    let mut s = finding();
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    s.launch.input_focus = 1;
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert!(s.launch.error.is_some_and(|e| e.contains("Milestones")));
}

/// Out of scope (#1290): the idea inbox is drawn disabled and cannot run.
#[test]
fn launch_input_idea_inbox_cannot_be_chosen() {
    let mut s = finding();
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    s.launch.input_focus = 3;
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_step(), LaunchStep::Input);
    assert_eq!(s.launch.error, Some(super::launch::IDEA_INBOX_DISABLED));
    assert_ne!(s.launch_payload().input_kind, LaunchInputKind::IdeaInbox);
}

// ── #877 — launch dispatch wiring ───────────────────────────────────────

#[test]
fn launch_dispatch_returns_launch_team_action_with_issue_input() {
    let mut s = planned_issue(42);
    s.set_launch_phase_for_test(LaunchPhase::Dialog);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(
        action,
        ScreenAction::LaunchTeam {
            team_name: "default-coder".into(),
            input: TeamInput::Issue { number: 42 },
            max_parallel: 3,
        }
    );
    assert_eq!(s.launch_phase(), LaunchPhase::Executing);
}

#[test]
fn launch_dispatch_without_a_plan_returns_none() {
    let mut s = fresh();
    s.apply_resolved_teams(vec![pipeline_team()]);
    s.set_launch_team_for_test("default-coder");
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_phase_for_test(LaunchPhase::Dialog);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_phase(), LaunchPhase::Dialog);
}

#[test]
fn launch_dispatch_no_team_selected_returns_none() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_phase_for_test(LaunchPhase::Dialog);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_phase(), LaunchPhase::Dialog);
}

#[test]
fn launch_dialog_esc_returns_to_the_plan_without_spending() {
    let mut s = planned_issue(42);
    s.set_launch_phase_for_test(LaunchPhase::Dialog);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_phase(), LaunchPhase::Editing);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
}

#[test]
fn esc_on_executing_step_is_swallowed() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_phase_for_test(LaunchPhase::Executing);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_phase(), LaunchPhase::Executing);
    assert_eq!(s.mode(), TeamWizardMode::Launch);
}

#[test]
fn esc_on_launch_success_pops_screen_does_not_retreat() {
    // Regression: Esc on "Launched" used to walk back into Executing,
    // stranding the wizard. Esc on a result must Pop.
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_phase_for_test(LaunchPhase::Launched);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
    assert_eq!(s.launch_phase(), LaunchPhase::Launched);
}

#[test]
fn esc_on_launch_failed_pops_screen_does_not_retreat() {
    let mut s = fresh();
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_phase_for_test(LaunchPhase::Failed);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
    assert_eq!(s.launch_phase(), LaunchPhase::Failed);
}

#[test]
fn r_on_launch_failed_goes_back_to_the_plan_and_never_spends() {
    let mut s = planned_issue(42);
    s.apply_launch_result(Err("boom".into()));
    let action = s.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.launch_phase(), LaunchPhase::Editing);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
}

// ── #875 — Ctrl+V paste behaviour ───────────────────────────────────────

fn with_clipboard(text: Option<&str>) -> TeamWizardScreen {
    use super::clipboard::testing::StubClipboard;
    let clip = match text {
        Some(t) => StubClipboard::with_text(t),
        None => StubClipboard::empty(),
    };
    TeamWizardScreen::with_clipboard_for_test(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        None,
        Box::new(clip),
    )
}

fn ctrl_v(s: &mut TeamWizardScreen) {
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
        InputMode::Insert,
    );
}

#[test]
fn ctrl_v_paste_populates_issue_buffer() {
    let mut s = with_clipboard(Some("#777"));
    s.set_launch_manual_issue_input_for_test("");
    ctrl_v(&mut s);
    assert_eq!(s.issue_input.text(), "777");
}

#[test]
fn ctrl_v_paste_non_numeric_leaves_buffer_unchanged() {
    let mut s = with_clipboard(Some("abc"));
    s.set_launch_manual_issue_input_for_test("");
    ctrl_v(&mut s);
    assert_eq!(s.issue_input.text(), "");
}

#[test]
fn ctrl_v_paste_with_empty_clipboard_is_noop() {
    let mut s = with_clipboard(None);
    s.set_launch_manual_issue_input_for_test("99");
    ctrl_v(&mut s);
    assert_eq!(s.issue_input.text(), "99");
}

#[test]
fn ctrl_v_paste_ignored_outside_the_find_field() {
    let mut s = with_clipboard(Some("#42"));
    let before_step = s.launch_step();
    ctrl_v(&mut s);
    assert_eq!(s.launch_step(), before_step);
    assert_eq!(s.issue_input.text(), "");
}

#[test]
fn ctrl_v_paste_truncates_long_issue_number() {
    let mut s = with_clipboard(Some("12345678901234"));
    s.set_launch_manual_issue_input_for_test("");
    ctrl_v(&mut s);
    assert_eq!(s.issue_input.text(), "1234567890");
}

// ── #876 — matches + key arms ───────────────────────────────────────────

#[test]
fn autocomplete_candidates_empty_buffer_returns_empty() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10]));
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn autocomplete_candidates_filters_by_prefix() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10, 12, 100, 101, 200]));
    s.set_launch_manual_issue_input_for_test("1");
    assert_eq!(s.autocomplete_candidates(), vec![1, 10, 12, 100, 101]);
    s.set_launch_manual_issue_input_for_test("#10");
    assert_eq!(s.autocomplete_candidates(), vec![10, 100, 101]);
    s.set_launch_manual_issue_input_for_test("5");
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn autocomplete_candidates_excludes_closed_issues() {
    let mut s = finding();
    let mut metas = std::collections::HashMap::new();
    metas.insert(42u64, make_issue_meta(42, IssueState::Closed, None, &[]));
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("4");
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn autocomplete_candidates_top_five_only() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10, 11, 12, 13, 14, 15, 100, 101, 111]));
    s.set_launch_manual_issue_input_for_test("1");
    assert_eq!(s.autocomplete_candidates(), vec![1, 10, 11, 12, 13]);
}

#[test]
fn autocomplete_candidates_with_no_metas_returns_empty() {
    let mut s = finding();
    s.set_launch_manual_issue_input_for_test("42");
    assert!(s.autocomplete_candidates().is_empty());
}

#[test]
fn arrow_down_moves_autocomplete_focus() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10, 12]));
    s.set_launch_manual_issue_input_for_test("1");
    assert_eq!(s.launch_payload().autocomplete_focus, None);
    s.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(0));
    s.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(1));
}

#[test]
fn arrow_up_clamps_at_zero() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10, 12]));
    s.set_launch_manual_issue_input_for_test("1");
    s.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    s.handle_input(&key_event(KeyCode::Up), InputMode::Insert);
    assert_eq!(s.launch_payload().autocomplete_focus, Some(0));
}

#[test]
fn autocomplete_focus_stays_none_on_down_when_no_candidates() {
    let mut s = finding();
    s.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    assert_eq!(s.launch_payload().autocomplete_focus, None);
}

/// §02.17: Tab accept is retired; Enter picks the focused match.
#[test]
fn enter_picks_the_focused_match() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[42, 420]));
    s.set_launch_manual_issue_input_for_test("42");
    s.set_autocomplete_focus_for_test(Some(1));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_payload().manual_issues, vec![420u64]);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
}

#[test]
fn enter_without_focus_takes_the_typed_number() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[42, 420]));
    s.set_launch_manual_issue_input_for_test("42");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_payload().manual_issues, vec![42u64]);
}

#[test]
fn backspace_resets_autocomplete_focus() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10, 12]));
    s.set_launch_manual_issue_input_for_test("12");
    s.set_autocomplete_focus_for_test(Some(2));
    s.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
    assert_eq!(s.launch_payload().autocomplete_focus, None);
    assert_eq!(s.issue_input.text(), "1");
}

#[test]
fn digit_keystroke_resets_autocomplete_focus() {
    let mut s = finding();
    s.apply_issue_metas(metas_open(&[1, 10, 12]));
    s.set_launch_manual_issue_input_for_test("1");
    s.set_autocomplete_focus_for_test(Some(0));
    s.handle_input(&key_event(KeyCode::Char('0')), InputMode::Insert);
    assert_eq!(s.launch_payload().autocomplete_focus, None);
    assert_eq!(s.issue_input.text(), "10");
}
