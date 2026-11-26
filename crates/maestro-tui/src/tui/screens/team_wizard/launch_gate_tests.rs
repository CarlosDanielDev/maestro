//! #1290 judge round: blockers come from the real Issues event (each body's
//! `## Blocked By`), a closed issue is never planned, and the launch dialog
//! never launches on the pre-spawn gate's projected Block (over the budget
//! left: `launch_tests::team_launch_over_budget_locked`, #1448).

use super::test_helpers::{make_issue_meta, make_test_team};
use super::types::{LaunchInputKind, LaunchPhase, LaunchStep, TeamLaunchInput, TeamWizardMode};
use super::{Screen, ScreenAction, TeamWizardScreen};
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::provider::types::Issue;
use crate::tui::app::{App, TuiCommand, TuiDataEvent, TuiMode};
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crate::tui::widgets::launch_dialog::LaunchBudget;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::HashMap;
use std::time::Instant;

fn team() -> crate::orchestration::team::ResolvedTeam {
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

fn issue(number: u64, state: &str, body: &str) -> Issue {
    Issue {
        number,
        title: format!("issue {number}"),
        body: body.to_string(),
        labels: Vec::new(),
        state: state.to_string(),
        html_url: String::new(),
        milestone: Some(1),
        assignees: Vec::new(),
    }
}

fn frame(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The production path: Milestones `t` seeds #11 and #12, the plan is on
/// screen before any data, then the real `TuiDataEvent::Issues` lands.
#[tokio::test]
async fn team_launch_reads_blocked_by_from_the_issues_event() {
    let mut app = crate::tui::make_test_app("team-launch-blocked-by");
    app.tui_mode = TuiMode::TeamWizard;
    let mut s = TeamWizardScreen::with_entry(
        Default::default(),
        TeamWizardMode::Launch,
        Some(TeamLaunchInput::Milestone {
            number: 1,
            title: "v1".into(),
            seed_issues: vec![11, 12],
        }),
    );
    s.apply_resolved_teams(vec![team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_team_for_test("default-coder");
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    assert_eq!(s.launch_payload().plan.as_ref().unwrap().final_count, 0);
    app.screen_state.team_wizard_screen = Some(s);

    // #11 waits on open #10; #12 is blocked by #77, which no fetch returned.
    app.handle_data_event(TuiDataEvent::Issues(Ok(vec![
        issue(10, "OPEN", "## Overview\n\nbase\n"),
        issue(11, "OPEN", "## Blocked By\n\n- #10 base\n"),
        issue(12, "OPEN", "## Blocked By\n\n- #77 elsewhere\n\n## Done\n"),
    ])));

    let plan = (app.screen_state.team_wizard_screen.as_ref())
        .and_then(|s| s.launch_payload().plan.clone())
        .expect("plan rebuilt on new data");
    assert_eq!(plan.levels, vec![vec![10], vec![11]]);
    assert_eq!(plan.deferred.len(), 1);
    assert_eq!(plan.deferred[0].0, 12);
    let screen = frame(&mut app);
    assert!(screen.contains("WAVE 2"), "{screen}");
    assert!(
        screen.contains("blocked by #77 (no issue data, state unknown)"),
        "{screen}"
    );
    assert!(!screen.contains("blocked by open #77"), "{screen}");

    // Plan ⏎ → dialog, dialog ⏎ → the dispatcher queues the run without #12.
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    crate::tui::input_handler::handle_key(&mut app, enter).await;
    crate::tui::input_handler::handle_key(&mut app, enter).await;
    let levels = app
        .pending_commands
        .iter()
        .find_map(|c| match c {
            TuiCommand::RunTeam { scheduler, .. } => Some(scheduler.levels().to_vec()),
            _ => None,
        })
        .expect("RunTeam queued");
    assert_eq!(levels, vec![vec![10], vec![11]], "#12 is never dispatched");
}

/// A typed number the cache has as closed is listed, never planned.
#[test]
fn team_launch_never_plans_a_closed_issue() {
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_resolved_teams(vec![team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_team_for_test("default-coder");
    let metas: HashMap<u64, IssueMeta> = [(5, make_issue_meta(5, IssueState::Closed, None, &[]))]
        .into_iter()
        .collect();
    s.apply_issue_metas(metas);
    s.set_launch_manual_issue_input_for_test("5");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(s.launch_step(), LaunchStep::Plan);
    let plan = s.launch_payload().plan.clone().unwrap();
    assert_eq!(plan.final_count, 0);
    assert!(plan.levels.is_empty());
    assert_eq!(plan.closed, vec![5]);

    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    t.draw(|f| s.draw(f, f.area(), &crate::tui::theme::Theme::dark()))
        .unwrap();
    let drawn: String = (t.backend().buffer().content().iter())
        .map(|c| c.symbol())
        .collect();
    assert!(drawn.contains("✓ already closed"), "{drawn}");

    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_phase(), LaunchPhase::Editing, "no dialog");
}

/// Launch on the dialog for #10 #11 #12 with `budget`, clock pinned.
fn dialog(budget: LaunchBudget, now: Instant) -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_resolved_teams(vec![team()]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_budget(budget);
    s.set_clock_for_test(now);
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(vec![10, 11, 12]);
    let metas: HashMap<u64, IssueMeta> = [10, 11, 12]
        .iter()
        .map(|n| (*n, make_issue_meta(*n, IssueState::Open, Some(1), &[])))
        .collect();
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_phase(), LaunchPhase::Dialog);
    s
}

fn budget(total_usd: f64, spent: f64) -> LaunchBudget {
    LaunchBudget {
        per_session_usd: 5.0,
        total_usd,
        spent,
        ..Default::default()
    }
}

fn enter(s: &mut TeamWizardScreen) -> ScreenAction {
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal)
}

/// The pre-spawn gate's projected Block locks the dialog like the shared one.
#[test]
fn team_launch_blocked_by_the_pre_spawn_gate_never_launches() {
    let mut s = dialog(budget(12.0, 12.0), Instant::now());
    assert!(s.launch_block_reason().is_some());
    for _ in 0..3 {
        assert_eq!(enter(&mut s), ScreenAction::None);
    }
    assert_eq!(s.launch_phase(), LaunchPhase::Dialog);
    assert_eq!(s.footer_hints(), vec![("Esc", "back to plan")]);
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    t.draw(|f| s.draw(f, f.area(), &crate::tui::theme::Theme::dark()))
        .unwrap();
    let drawn: String = (t.backend().buffer().content().iter())
        .map(|c| c.symbol())
        .collect();
    assert!(drawn.contains("over the cap"), "{drawn}");
}
