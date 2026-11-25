//! #1284 judge round (#1451): no key during a write, the retry after a
//! partial create (what exists, reuse by number), the planning-failure
//! keys, Help for every footer key, and quit during a write. Helpers are
//! shared with `create_tests.rs`. No real GitHub writes: creates are only
//! queued, and `materialize_plan_with_client` runs against
//! `MockGitHubClient`.

use super::super::{
    AiGeneratedPlan, MilestoneCreationResult, MilestoneWizardScreen, MilestoneWizardStep,
    PlanWritten, materialize_plan_with_client,
};
use super::tests::{
    app_on, chain_plan, create_queued, creates_queued, issue, plannings_queued, press, screen_text,
    wizard,
};
use crate::tui::app::{App, TuiCommand, TuiMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

#[tokio::test]
async fn esc_after_a_partial_create_goes_to_create_not_complete() {
    let mut app = app_on(MilestoneWizardStep::Materializing, chain_plan());
    wizard(&mut app).finish_materialization(Ok(partial_after_schema()));
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    assert!(!create_queued(&app));
}

#[tokio::test]
async fn one_confirm_queues_one_create_across_ticks() {
    let mut app = app_on(MilestoneWizardStep::Preview, chain_plan());
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(creates_queued(&app), 0, "arming queues nothing");
    wizard(&mut app).set_clock_for_test(t0 + Duration::from_secs(1));
    press(&mut app, KeyCode::Enter).await;
    crate::tui::screen_dispatch::tick_wizard_step_hooks(&mut app);
    crate::tui::screen_dispatch::tick_wizard_step_hooks(&mut app);
    assert_eq!(creates_queued(&app), 1, "one confirm, one create");
}

#[tokio::test]
async fn entering_ai_structuring_spends_nothing_until_enter() {
    let mut screen = MilestoneWizardScreen::new();
    screen.set_step_for_tests(MilestoneWizardStep::DocReferences);
    let mut app = crate::tui::make_test_app("milestone-wizard-1284-plan");
    app.tui_mode = TuiMode::MilestoneWizard;
    app.screen_state.milestone_wizard_screen = Some(screen);
    crate::tui::input_handler::handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
    )
    .await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::AiStructuring);
    crate::tui::screen_dispatch::tick_wizard_step_hooks(&mut app);
    assert_eq!(plannings_queued(&app), 0, "entering the step never spends");

    press(&mut app, KeyCode::Enter).await;
    crate::tui::screen_dispatch::tick_wizard_step_hooks(&mut app);
    assert_eq!(plannings_queued(&app), 1, "one ⏎, one planner run");
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::AiStructuring);
    assert!(wizard(&mut app).is_planning_in_flight());
}

/// The chain plan's partial create: milestone #9, P0 written as #31, then
/// P1 failed.
fn partial_after_schema() -> MilestoneCreationResult {
    MilestoneCreationResult::Partial {
        milestone_number: 9,
        milestone_existed: false,
        issue_numbers: vec![31],
        existed: vec![],
        not_created: vec!["docs: guide".into(), "feat: api".into()],
        written: PlanWritten {
            milestone_number: 9,
            issues: [(0, 31)].into_iter().collect(),
        },
        error: "create_issue 'docs: guide' failed: HTTP 502".into(),
    }
}

fn queued_plan(app: &App) -> Option<&AiGeneratedPlan> {
    app.pending_commands.iter().find_map(|c| match c {
        TuiCommand::CreateMilestoneWithIssues(p) => Some(p),
        _ => None,
    })
}

/// Judge #1451: a write in flight cannot be left or started twice.
#[tokio::test]
async fn keys_during_a_write_change_nothing_and_start_no_second_create() {
    let mut app = app_on(MilestoneWizardStep::Preview, chain_plan());
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Materializing);
    assert_eq!(creates_queued(&app), 1);

    press(&mut app, KeyCode::Esc).await;
    assert_eq!(
        wizard(&mut app).step(),
        MilestoneWizardStep::Materializing,
        "Esc never leaves a write in flight"
    );
    for _ in 0..2 {
        press(&mut app, KeyCode::Enter).await;
    }
    crate::tui::screen_dispatch::tick_wizard_step_hooks(&mut app);
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Materializing);
    assert_eq!(creates_queued(&app), 1, "still one create for one confirm");
    let screen = screen_text(&mut app);
    assert!(screen.contains("writing… wait"), "says to wait:\n{screen}");
}

/// Judge #1451: after a partial create, Create says what exists and what
/// the retry does, and the retry carries the numbers.
#[tokio::test]
async fn create_after_a_partial_states_what_exists_and_reuses_it() {
    let mut app = app_on(MilestoneWizardStep::Materializing, chain_plan());
    wizard(&mut app).finish_materialization(Ok(partial_after_schema()));
    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    let screen = screen_text(&mut app);
    assert!(screen.contains("1 of 3 issues exist"), "ledger:\n{screen}");
    assert!(
        !screen.contains("nothing written yet"),
        "never claims nothing exists:\n{screen}"
    );
    assert!(
        screen.contains("creates 2 issues · reuses milestone #9 and 1 issue"),
        "the bar says what the retry does:\n{screen}"
    );

    press(&mut app, KeyCode::Enter).await;
    wizard(&mut app).set_clock_for_test(t0 + Duration::from_secs(1));
    press(&mut app, KeyCode::Enter).await;
    let written = queued_plan(&app).and_then(|p| p.written.clone());
    assert_eq!(
        written,
        Some(PlanWritten {
            milestone_number: 9,
            issues: [(0, 31)].into_iter().collect(),
        }),
        "the retry's plan carries what exists"
    );
}

/// Judge #1451: a retry after a partial reuses the milestone and the
/// written issues by number and creates only the missing ones.
#[tokio::test]
async fn retry_after_partial_creates_only_the_missing_issues() {
    use crate::provider::github::client::mock::MockGitHubClient;

    // P0 and P2 are level 0, P1 (blocked by P0) is level 1 and fails.
    let mut plan = AiGeneratedPlan {
        milestone_title: "M3: Retry".into(),
        milestone_description: "d".into(),
        issues: vec![
            issue("feat: one", vec![]),
            issue("", vec![0]),
            issue("feat: three", vec![]),
        ],
        written: None,
    };
    let client = MockGitHubClient::new();
    let first = materialize_plan_with_client(&plan, &client).await.unwrap();
    let MilestoneCreationResult::Partial {
        milestone_number,
        written,
        ..
    } = first
    else {
        panic!("expected Partial, got {first:?}");
    };
    let one = written.issues[&0];

    // The failure is fixed (here: the title), and the retry runs. P0's
    // title changed too: a retry that matched by title would create P0
    // again; reuse by number does not.
    plan.issues[1].title = "feat: two".into();
    plan.issues[0].title = "feat: one, renamed".into();
    plan.written = Some(written);
    let second = materialize_plan_with_client(&plan, &client).await.unwrap();

    assert_eq!(client.create_milestone_calls().len(), 1, "milestone reused");
    let calls: Vec<String> = client
        .create_issue_calls()
        .iter()
        .map(|c| c.title.clone())
        .collect();
    // Reuse is by plan index → number, not by title: run 2 calls only P1.
    assert_eq!(calls, ["feat: one", "feat: three", "feat: two"]);
    let MilestoneCreationResult::Reused {
        milestone_number: m2,
        new_issue_numbers,
        skipped_issue_numbers,
    } = second
    else {
        panic!("expected Reused, got {second:?}");
    };
    assert_eq!(m2, milestone_number);
    assert!(skipped_issue_numbers.contains(&one), "P0 reused by number");
    assert_eq!(
        skipped_issue_numbers.len(),
        2,
        "P0 and P2 were already there"
    );
    assert_eq!(new_issue_numbers.len(), 1, "only P1 is created");
    let p1_body = client.create_issue_calls().last().unwrap().body.clone();
    assert!(
        p1_body.contains(&format!("- #{one}")),
        "P1's Blocked By names P0's real number: {p1_body}"
    );
}

/// Judge #1451 nit: after a planning failure the shown keys act.
#[tokio::test]
async fn a_planning_failure_can_plan_again() {
    let mut screen = MilestoneWizardScreen::new();
    screen.set_step_for_tests(MilestoneWizardStep::AiStructuring);
    screen.start_planning();
    screen.apply_planning_result(Err("claude: exit 1".into()));
    let mut app = crate::tui::make_test_app("milestone-wizard-1284-replan");
    app.tui_mode = TuiMode::MilestoneWizard;
    app.screen_state.milestone_wizard_screen = Some(screen);
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Failed);

    press(&mut app, KeyCode::Esc).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::AiStructuring);
    assert_eq!(plannings_queued(&app), 0, "Esc never spends");
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(plannings_queued(&app), 1, "⏎ runs the planner again");

    wizard(&mut app).apply_planning_result(Err("claude: exit 1".into()));
    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::AiStructuring);
    assert_eq!(plannings_queued(&app), 2, "r asks the planner again");
}

/// Judge #1451: every footer key is in Help for this screen.
#[test]
fn every_footer_key_is_in_help() {
    use crate::tui::navigation::keymap::{KeymapProvider, global_keybindings, mode_help_groups};
    let s = MilestoneWizardScreen::new();
    let mut help: Vec<String> = mode_help_groups(TuiMode::MilestoneWizard)
        .iter()
        .chain(global_keybindings())
        .chain(s.keybindings().iter())
        .flat_map(|g| g.bindings.iter().map(|b| b.key.to_string()))
        // `j / ↓` names two keys.
        .flat_map(|k| k.split(" / ").map(str::to_string).collect::<Vec<_>>())
        .collect();
    help.sort();
    let mut s = MilestoneWizardScreen::new();
    s.apply_planning_result(Ok(chain_plan()));
    for step in MilestoneWizardStep::ALL {
        s.set_step_for_tests(*step);
        for (keys, _) in super::super::draw::footer_hints(&s) {
            for key in keys.split(' ') {
                assert!(
                    help.iter().any(|h| h == key),
                    "{step:?}: footer key {key:?} is not in Help {help:?}"
                );
            }
        }
    }
}

/// A write in flight is live work: the quit chords still reach Confirm
/// Exit (they run before the screen), and ⌃Q asks instead of quitting.
#[tokio::test]
async fn quit_during_a_write_asks_first() {
    for c in ['q', 'c'] {
        let mut app = app_on(MilestoneWizardStep::Preview, chain_plan());
        let t0 = Instant::now();
        wizard(&mut app).set_clock_for_test(t0);
        press(&mut app, KeyCode::Enter).await;
        press(&mut app, KeyCode::Enter).await;
        assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Materializing);
        assert!(crate::tui::input_handler::has_live_work(&app));
        crate::tui::input_handler::handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
        )
        .await;
        assert!(app.running, "⌃{c} never quits during a write");
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "⌃{c} asks first");
    }
}
