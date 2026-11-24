//! Snapshot tests for the Team Wizard. Mandatory three-size fan-out at the
//! flow heads; terminal states (Success / Failed) snapshot at the default
//! 80×24 only to keep the corpus tractable.
//!
//! Naming convention: `<flow>_<step>[_<variant>]_<width>x<height>`.

use crate::orchestration::dag::IssueState;
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::provider::types::ProviderKind;
use crate::tui::screens::Screen;
use crate::tui::screens::team_wizard::test_helpers::{
    make_health_check as make_health, make_issue_meta as make_meta, make_test_team as make_team,
};
use crate::tui::screens::team_wizard::{
    LaunchInputKind, LaunchPhase, LaunchStep, ManageStep, TeamLaunchInput, TeamWizardMode,
    TeamWizardScreen,
    types::{ComposeSource, ComposeStep},
};
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use std::collections::HashMap;

const SIZES: &[(u16, u16)] = &[(60, 20), (80, 24), (120, 40)];

fn draw_team_wizard(
    screen: &mut TeamWizardScreen,
    width: u16,
    height: u16,
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();
    terminal
}

// ── Home ────────────────────────────────────────────────────────────────

#[test]
fn home_60x20() {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    let t = draw_team_wizard(&mut s, 60, 20);
    assert_snapshot!(t.backend());
}

#[test]
fn home_80x24() {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn home_120x40() {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    let t = draw_team_wizard(&mut s, 120, 40);
    assert_snapshot!(t.backend());
}

/// #1286: the overview groups presets by tier with readiness, and draws
/// the selected preset's card beside the list.
#[test]
fn teams_overview_wide() {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    s.apply_resolved_teams(vec![
        make_team(
            "default-coder",
            Primitive::Pipeline,
            &[
                (TeamRole::Implementer, "claude"),
                (TeamRole::Reviewer, "claude"),
                (TeamRole::Docs, "claude"),
            ],
            SourceTier::BuiltIn,
        ),
        make_team(
            "default-researcher",
            Primitive::VerdictOnly,
            &[(TeamRole::Reviewer, "claude")],
            SourceTier::BuiltIn,
        ),
        make_team(
            "cheap-coder",
            Primitive::Pipeline,
            &[
                (TeamRole::Implementer, "minimax"),
                (TeamRole::Reviewer, "claude"),
                (TeamRole::Docs, "claude"),
            ],
            SourceTier::User,
        ),
        make_team(
            "issue-\x1b[31msanitizer",
            Primitive::SinglePass,
            &[(TeamRole::Reviewer, "claude")],
            SourceTier::Project,
        ),
    ]);
    s.apply_health_check(vec![
        make_health("claude", true),
        make_health("minimax", false),
    ]);
    let t = draw_team_wizard(&mut s, 168, 42);
    assert_snapshot!(t.backend());
}

// ── Compose ─────────────────────────────────────────────────────────────

fn populated_compose_screen() -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    s.apply_resolved_teams(vec![
        make_team(
            "default-coder",
            Primitive::Pipeline,
            &[
                (TeamRole::Implementer, "claude"),
                (TeamRole::Reviewer, "claude"),
                (TeamRole::Docs, "claude"),
            ],
            SourceTier::BuiltIn,
        ),
        make_team("user-team", Primitive::SinglePass, &[], SourceTier::User),
    ]);
    s.apply_health_check(vec![
        make_health("claude", true),
        make_health("codex", false),
    ]);
    s.set_preset_dirs_for_test("/home/u/.config/maestro/teams".into(), "/repo".into());
    s.switch_mode(TeamWizardMode::Compose);
    s
}

#[test]
fn compose_source_60x20() {
    let mut s = populated_compose_screen();
    let t = draw_team_wizard(&mut s, 60, 20);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_source_80x24() {
    let mut s = populated_compose_screen();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_source_120x40() {
    let mut s = populated_compose_screen();
    let t = draw_team_wizard(&mut s, 120, 40);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_primitive_80x24() {
    let mut s = populated_compose_screen();
    s.set_compose_step_for_test(ComposeStep::Primitive);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_roles_80x24() {
    let mut s = populated_compose_screen();
    s.set_compose_primitive_for_test(Primitive::Pipeline);
    s.set_compose_step_for_test(ComposeStep::Roles);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

/// Review & save with a full draft: the path and every TOML line it writes.
fn ready_to_save_screen() -> TeamWizardScreen {
    let mut s = populated_compose_screen();
    s.set_compose_source_for_test(ComposeSource::Extends("default-coder".into()));
    s.set_compose_primitive_for_test(Primitive::Pipeline);
    for role in [TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs] {
        s.set_compose_binding_for_test(role, "claude");
    }
    s.set_compose_name_for_test("my-team");
    s
}

#[test]
fn compose_save_80x24() {
    let mut s = ready_to_save_screen();
    s.set_compose_step_for_test(ComposeStep::Save);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_save_success_80x24() {
    let mut s = ready_to_save_screen();
    s.set_compose_save_result_for_test(Ok(()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_save_failed_80x24() {
    let mut s = ready_to_save_screen();
    s.set_compose_save_result_for_test(Err("name already exists".into()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

// ── Launch ──────────────────────────────────────────────────────────────

fn populated_launch_screen() -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    s.apply_resolved_teams(vec![make_team(
        "default-coder",
        Primitive::Pipeline,
        &[
            (TeamRole::Implementer, "claude"),
            (TeamRole::Reviewer, "claude"),
            (TeamRole::Docs, "claude"),
        ],
        SourceTier::BuiltIn,
    )]);
    s.apply_health_check(vec![make_health("claude", true)]);
    s.set_known_agents(vec!["claude".into()]);
    s.switch_mode(TeamWizardMode::Launch);
    s
}

#[test]
fn launch_team_picker_60x20() {
    let mut s = populated_launch_screen();
    let t = draw_team_wizard(&mut s, 60, 20);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_team_picker_80x24() {
    let mut s = populated_launch_screen();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_team_picker_120x40() {
    let mut s = populated_launch_screen();
    let t = draw_team_wizard(&mut s, 120, 40);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_input_picker_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_step_for_test(LaunchStep::Input);
    s.set_launch_team_for_test("default-coder");
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_issue_picker_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_team_for_test("default-coder");
    s.set_launch_manual_issue_input_for_test("");
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_issue_picker_autocomplete_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_team_for_test("default-coder");
    let mut metas = HashMap::new();
    for n in [1u64, 10, 12, 100, 101, 200] {
        metas.insert(n, make_meta(n, IssueState::Open, Some(1), &[]));
    }
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("1");
    s.set_autocomplete_focus_for_test(Some(0));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_plan_preview_green_80x24() {
    let mut s = TeamWizardScreen::with_entry(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        Some(TeamLaunchInput::Issue {
            number: 42,
            title: "feat: login".into(),
        }),
    );
    s.apply_resolved_teams(vec![make_team(
        "default-coder",
        Primitive::Pipeline,
        &[
            (TeamRole::Implementer, "claude"),
            (TeamRole::Reviewer, "claude"),
            (TeamRole::Docs, "claude"),
        ],
        SourceTier::BuiltIn,
    )]);
    s.apply_health_check(vec![make_health("claude", true)]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    let mut metas = HashMap::new();
    metas.insert(42, make_meta(42, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_input_for_test(LaunchInputKind::Issue, Some(42));
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_plan_preview_blocked_role_80x24() {
    let mut s = populated_launch_screen();
    let team = make_team("partial", Primitive::Pipeline, &[], SourceTier::User);
    s.apply_resolved_teams(vec![team]);
    s.set_launch_team_for_test("partial");
    s.set_launch_input_for_test(LaunchInputKind::Issue, Some(1));
    let mut metas = HashMap::new();
    metas.insert(1, make_meta(1, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_plan_preview_open_external_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(vec![1]);
    let mut metas = HashMap::new();
    metas.insert(1, make_meta(1, IssueState::Open, Some(7), &[2]));
    metas.insert(2, make_meta(2, IssueState::Open, Some(7), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_confirm_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::Issue, Some(42));
    let mut metas = HashMap::new();
    metas.insert(42, make_meta(42, IssueState::Open, Some(1), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_budget(budget());
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    s.set_launch_phase_for_test(LaunchPhase::Dialog);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_executing_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.set_launch_phase_for_test(LaunchPhase::Executing);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

/// #1246: wave 1 finished, wave 2 running, wave 3 waits on wave 2.
#[test]
fn team_result_waiting_wave() {
    let mut s = populated_launch_screen();
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(vec![10, 11, 12, 13]);
    let mut metas = HashMap::new();
    metas.insert(10, make_meta(10, IssueState::Open, Some(1), &[]));
    metas.insert(11, make_meta(11, IssueState::Open, Some(1), &[10]));
    metas.insert(12, make_meta(12, IssueState::Open, Some(1), &[10]));
    metas.insert(13, make_meta(13, IssueState::Open, Some(1), &[11]));
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    s.set_launch_phase_for_test(LaunchPhase::Executing);
    s.apply_wave_started(1);
    let t = draw_team_wizard(&mut s, 80, 24);
    let drawn: String = t
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        drawn.contains("waiting on wave 2"),
        "wave 3 must wait on wave 2"
    );
    assert!(drawn.contains("running"), "wave 2 must read running");
    assert_snapshot!(t.backend());
}

#[test]
fn launch_success_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.apply_launch_result(Ok(()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_failed_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.apply_launch_result(Err("dispatch error".into()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

/// The launch dialog's budget: $5.00 per session, $50.00 total, $12.00 spent.
fn budget() -> crate::tui::widgets::launch_dialog::LaunchBudget {
    crate::tui::widgets::launch_dialog::LaunchBudget {
        per_session_usd: 5.0,
        total_usd: 50.0,
        spent: 12.0,
        ..Default::default()
    }
}

/// #1290: three steps, the Plan as `run_plan` waves of lanes with the
/// auto-added blocker, the held issue under NOT INCLUDED, and the exact
/// worst case against the budget left. Wide shows the Preflight column.
#[test]
fn team_launch_plan_wide() {
    // Opened from Milestones `t` on milestone 1 with four of its issues.
    let mut s = TeamWizardScreen::with_entry(
        ProviderKind::default(),
        TeamWizardMode::Launch,
        Some(TeamLaunchInput::Milestone {
            number: 1,
            title: "v1".into(),
            seed_issues: vec![11, 12, 13, 14],
        }),
    );
    s.apply_resolved_teams(
        populated_launch_screen()
            .resolved_teams()
            .values()
            .cloned()
            .collect(),
    );
    s.apply_health_check(vec![make_health("claude", true)]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_budget(budget());
    s.set_launch_team_for_test("default-coder");
    let mut metas = HashMap::new();
    metas.insert(10, make_meta(10, IssueState::Open, Some(1), &[]));
    metas.insert(11, make_meta(11, IssueState::Open, Some(1), &[10]));
    metas.insert(12, make_meta(12, IssueState::Open, Some(1), &[10]));
    metas.insert(13, make_meta(13, IssueState::Open, Some(1), &[11]));
    metas.insert(14, make_meta(14, IssueState::Open, Some(1), &[99]));
    metas.insert(99, make_meta(99, IssueState::Open, Some(2), &[]));
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    let t = draw_team_wizard(&mut s, 168, 42);
    assert_snapshot!(t.backend());
}

// ── Manage ──────────────────────────────────────────────────────────────

fn populated_manage_screen() -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    s.apply_resolved_teams(vec![
        make_team(
            "default-coder",
            Primitive::Pipeline,
            &[],
            SourceTier::BuiltIn,
        ),
        make_team("my-team", Primitive::SinglePass, &[], SourceTier::User),
        make_team("fast-team", Primitive::FanOut, &[], SourceTier::User),
    ]);
    s.switch_mode(TeamWizardMode::Manage);
    s
}

#[test]
fn manage_list_populated_60x20() {
    let mut s = populated_manage_screen();
    let t = draw_team_wizard(&mut s, 60, 20);
    assert_snapshot!(t.backend());
}

#[test]
fn manage_list_populated_80x24() {
    let mut s = populated_manage_screen();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn manage_list_populated_120x40() {
    let mut s = populated_manage_screen();
    let t = draw_team_wizard(&mut s, 120, 40);
    assert_snapshot!(t.backend());
}

#[test]
fn manage_list_empty_80x24() {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    s.switch_mode(TeamWizardMode::Manage);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn manage_delete_confirm_80x24() {
    let mut s = populated_manage_screen();
    s.set_manage_step_for_test(ManageStep::DeleteConfirm);
    s.set_manage_pending_delete_for_test("my-team");
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn manage_delete_success_80x24() {
    let mut s = populated_manage_screen();
    s.apply_delete_result(Ok(()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn manage_delete_failed_80x24() {
    let mut s = populated_manage_screen();
    s.apply_delete_result(Err("permission denied".into()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

// Cover the SIZES const usage so the symbol isn't dead — this also fans
// out the launch input picker which is the most info-dense step.
#[test]
fn launch_input_picker_with_preselect_at_all_sizes() {
    for (w, h) in SIZES {
        let mut s = TeamWizardScreen::with_entry(
            ProviderKind::default(),
            TeamWizardMode::Launch,
            Some(TeamLaunchInput::Issue {
                number: 42,
                title: "feat: login".into(),
            }),
        );
        s.apply_resolved_teams(vec![make_team(
            "default-coder",
            Primitive::Pipeline,
            &[],
            SourceTier::BuiltIn,
        )]);
        s.set_launch_team_for_test("default-coder");
        s.set_launch_step_for_test(LaunchStep::Input);
        let t = draw_team_wizard(&mut s, *w, *h);
        assert_snapshot!(format!("launch_input_preselect_{w}x{h}"), t.backend());
    }
}
