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
    LaunchInputKind, LaunchStep, ManageStep, TeamLaunchInput, TeamWizardMode, TeamWizardScreen,
    types::ComposeStep,
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

#[test]
fn compose_overrides_80x24() {
    let mut s = populated_compose_screen();
    s.set_compose_step_for_test(ComposeStep::Overrides);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_save_80x24() {
    let mut s = populated_compose_screen();
    s.set_compose_step_for_test(ComposeStep::Save);
    s.set_compose_name_for_test("my-team");
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_save_success_80x24() {
    let mut s = populated_compose_screen();
    s.set_compose_step_for_test(ComposeStep::SaveSuccess);
    s.set_compose_name_for_test("my-team");
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn compose_save_failed_80x24() {
    let mut s = populated_compose_screen();
    s.apply_save_result(Err("name already exists".into()));
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
    s.set_launch_step_for_test(LaunchStep::InputPicker);
    s.set_launch_team_for_test("default-coder");
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
    s.set_launch_step_for_test(LaunchStep::PlanPreview);
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
    s.set_launch_step_for_test(LaunchStep::PlanPreview);
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
    s.set_launch_step_for_test(LaunchStep::PlanPreview);
    s.build_plan_preview_for_test();
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_confirm_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_team_for_test("default-coder");
    s.set_launch_step_for_test(LaunchStep::Confirm);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_executing_80x24() {
    let mut s = populated_launch_screen();
    s.set_launch_step_for_test(LaunchStep::Executing);
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_success_80x24() {
    let mut s = populated_launch_screen();
    s.apply_launch_result(Ok(()));
    let t = draw_team_wizard(&mut s, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn launch_failed_80x24() {
    let mut s = populated_launch_screen();
    s.apply_launch_result(Err("dispatch error".into()));
    let t = draw_team_wizard(&mut s, 80, 24);
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
        s.set_launch_step_for_test(LaunchStep::InputPicker);
        let t = draw_team_wizard(&mut s, *w, *h);
        assert_snapshot!(format!("launch_input_preselect_{w}x{h}"), t.backend());
    }
}
