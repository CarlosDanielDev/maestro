//! #1284: the Plan check (stranded issues block advancing), the armed
//! Create (⏎ twice within 3s, with the issue count), and the partial
//! create result. Key tests go through `handle_key` + `ui::draw` at 80x24
//! (the SizeGuard swallows keys below it). No real GitHub writes: the
//! creation command is only queued, and `materialize_plan_with_client`
//! runs against `MockGitHubClient`.

use super::super::{
    AiGeneratedPlan, AiProposedIssue, MilestoneCreationResult, MilestoneWizardScreen,
    MilestoneWizardStep, level_buckets, materialize_plan_with_client, stranded,
};
use super::{CYCLE_REASON, STRANDED_REASON};
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::ds::armed::ArmedState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

fn issue(title: &str, blocked_by: Vec<usize>) -> AiProposedIssue {
    AiProposedIssue {
        title: title.into(),
        overview: format!("{title} overview"),
        blocked_by,
        accepted: true,
    }
}

/// P0 schema ← P1 api, P2 guide (no blockers).
fn chain_plan() -> AiGeneratedPlan {
    AiGeneratedPlan {
        milestone_title: "M1: API".into(),
        milestone_description: "Ship the API.".into(),
        issues: vec![
            issue("feat: schema", vec![]),
            issue("feat: api", vec![0]),
            issue("docs: guide", vec![]),
        ],
    }
}

fn app_on(step: MilestoneWizardStep, plan: AiGeneratedPlan) -> App {
    let mut screen = MilestoneWizardScreen::new();
    screen.apply_planning_result(Ok(plan));
    screen.set_step_for_tests(step);
    let mut app = crate::tui::make_test_app("milestone-wizard-1284");
    app.tui_mode = TuiMode::MilestoneWizard;
    app.screen_state.milestone_wizard_screen = Some(screen);
    app
}

fn wizard(app: &mut App) -> &mut MilestoneWizardScreen {
    app.screen_state
        .milestone_wizard_screen
        .as_mut()
        .expect("wizard")
}

async fn press(app: &mut App, code: KeyCode) {
    crate::tui::input_handler::handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn screen_text(app: &mut App) -> String {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn create_queued(app: &App) -> bool {
    creates_queued(app) > 0
}

fn creates_queued(app: &App) -> usize {
    app.pending_commands
        .iter()
        .filter(|c| matches!(c, TuiCommand::CreateMilestoneWithIssues(_)))
        .count()
}

fn plannings_queued(app: &App) -> usize {
    app.pending_commands
        .iter()
        .filter(|c| matches!(c, TuiCommand::LaunchAiPlanning(_)))
        .count()
}

/// Done when: "deselecting a blocker marks its dependents stranded and
/// blocks advancing".
#[tokio::test]
async fn milestone_wizard_stranded_blocks_advance() {
    let mut app = app_on(MilestoneWizardStep::ReviewPlan, chain_plan());
    assert!(wizard(&mut app).stranded_issues().is_empty());

    // Focus starts on P0 (schema); x deselects the blocker of P1.
    press(&mut app, KeyCode::Char('x')).await;
    press(&mut app, KeyCode::Down).await;
    press(&mut app, KeyCode::Down).await;
    assert_eq!(wizard(&mut app).stranded_issues(), vec![(1, vec![0])]);
    assert_eq!(wizard(&mut app).validation_error(), Some(STRANDED_REASON));

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(
        wizard(&mut app).step(),
        MilestoneWizardStep::ReviewPlan,
        "⏎ never leaves Plan while an issue is stranded"
    );
    assert_eq!(
        wizard(&mut app).review_focus(),
        1,
        "⏎ focuses the first stranded issue"
    );
    let screen = screen_text(&mut app);
    assert!(screen.contains("[!]"), "stranded mark:\n{screen}");
    assert!(
        screen.contains("needs deselected \"feat: schema\""),
        "names the deselected blocker:\n{screen}"
    );

    // Dropping the dependent too clears the block, and ⏎ advances.
    press(&mut app, KeyCode::Char('x')).await;
    assert!(wizard(&mut app).stranded_issues().is_empty());
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    assert!(!create_queued(&app), "advancing to Create writes nothing");
}

#[test]
fn stranded_lists_out_of_range_blockers() {
    let issues = vec![issue("a", vec![7])];
    assert_eq!(stranded(&issues), vec![(0, vec![7])]);
}

#[test]
fn no_accepted_issue_is_dropped_when_nothing_is_stranded() {
    // P0 ← P1 ← P2, P3 ← P4, one deselected leaf.
    let mut issues = vec![
        issue("p0", vec![]),
        issue("p1", vec![0]),
        issue("p2", vec![1]),
        issue("p3", vec![]),
        issue("p4", vec![3]),
        issue("p5", vec![4]),
    ];
    issues[5].accepted = false;
    assert!(stranded(&issues).is_empty());
    let placed: usize = level_buckets(&issues).iter().map(Vec::len).sum();
    assert_eq!(placed, issues.iter().filter(|i| i.accepted).count());
}

#[test]
fn a_blocker_loop_blocks_advancing() {
    let plan = AiGeneratedPlan {
        milestone_title: "M".into(),
        milestone_description: String::new(),
        issues: vec![issue("a", vec![1]), issue("b", vec![0])],
    };
    let mut s = MilestoneWizardScreen::new();
    s.apply_planning_result(Ok(plan));
    s.set_step_for_tests(MilestoneWizardStep::ReviewPlan);
    assert_eq!(s.validation_error(), Some(CYCLE_REASON));
    assert!(!s.try_advance());
    assert_eq!(s.step(), MilestoneWizardStep::ReviewPlan);
}

/// Done when: "create needs ⏎ twice within 3s and states the issue count".
#[tokio::test]
async fn milestone_create_armed_counts() {
    let mut plan = chain_plan();
    plan.issues[2].accepted = false; // 2 of 3 selected
    let mut app = app_on(MilestoneWizardStep::Preview, plan);
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    let screen = screen_text(&mut app);
    assert!(screen.contains("READY"), "ready before ⏎:\n{screen}");

    press(&mut app, KeyCode::Enter).await;
    assert!(
        matches!(wizard(&mut app).create_arm_state(), ArmedState::Armed(_)),
        "first ⏎ arms"
    );
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    assert!(!create_queued(&app), "one ⏎ never writes");
    let screen = screen_text(&mut app);
    assert!(screen.contains("ARMED"), "armed bar:\n{screen}");
    assert!(
        screen.contains("creates 1 milestone + 2 issues"),
        "states the milestone and the issue count:\n{screen}"
    );

    wizard(&mut app).set_clock_for_test(t0 + Duration::from_secs(1));
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Materializing);
    assert_eq!(wizard(&mut app).materialize_progress(), Some((0, 2)));
    assert!(create_queued(&app), "second ⏎ within 3s queues the write");
}

#[tokio::test]
async fn late_second_enter_rearms_and_writes_nothing() {
    let mut app = app_on(MilestoneWizardStep::Preview, chain_plan());
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    press(&mut app, KeyCode::Enter).await;
    wizard(&mut app).set_clock_for_test(t0 + Duration::from_secs(4));
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    assert!(matches!(
        wizard(&mut app).create_arm_state(),
        ArmedState::Armed(_)
    ));
    assert!(!create_queued(&app));
}

#[tokio::test]
async fn esc_disarms_create_and_stays() {
    let mut app = app_on(MilestoneWizardStep::Preview, chain_plan());
    wizard(&mut app).set_clock_for_test(Instant::now());
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(*wizard(&mut app).create_arm_state(), ArmedState::Ready);
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    press(&mut app, KeyCode::Enter).await;
    assert!(
        !create_queued(&app),
        "a disarmed ⏎ arms again, never writes"
    );
}

/// Done when: "a failure after k issues returns Partial with those k
/// numbers".
#[tokio::test]
async fn materialize_partial_keeps_numbers() {
    use crate::provider::github::client::mock::MockGitHubClient;

    // Four independent issues; the mock rejects the empty title of the
    // third, so the create fails after k = 2 issues.
    let plan = AiGeneratedPlan {
        milestone_title: "M2: Partial".into(),
        milestone_description: "d".into(),
        issues: vec![
            issue("feat: one", vec![]),
            issue("feat: two", vec![]),
            issue("", vec![]),
            issue("feat: four", vec![]),
        ],
    };
    let client = MockGitHubClient::new();
    let result = materialize_plan_with_client(&plan, &client)
        .await
        .expect("a partial create is not an Err");

    let calls: Vec<String> = client
        .create_issue_calls()
        .iter()
        .map(|c| c.title.clone())
        .collect();
    assert_eq!(calls, ["feat: one", "feat: two"]);
    let MilestoneCreationResult::Partial {
        milestone_number,
        issue_numbers,
        not_created,
        error,
    } = result.clone()
    else {
        panic!("expected Partial, got {result:?}");
    };
    assert_eq!(
        issue_numbers.len(),
        2,
        "keeps the k numbers: {issue_numbers:?}"
    );
    assert_eq!(not_created, ["", "feat: four"]);
    assert!(error.contains("create_issue"), "{error}");

    // The screen keeps the numbers and says what was not created.
    let mut app = app_on(MilestoneWizardStep::Materializing, plan);
    wizard(&mut app).finish_materialization(Ok(result));
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Failed);
    assert_eq!(wizard(&mut app).created_issue_numbers(), issue_numbers);
    assert_eq!(
        wizard(&mut app).created_milestone_number(),
        Some(milestone_number)
    );
    let screen = screen_text(&mut app);
    let first = format!("#{}", issue_numbers[0]);
    assert!(screen.contains(&first), "created numbers shown:\n{screen}");
    assert!(
        screen.contains("not created: feat: four"),
        "what was not created:\n{screen}"
    );
    assert!(
        screen.contains("2 of 4 issues written"),
        "ledger:\n{screen}"
    );
}

#[tokio::test]
async fn esc_after_a_partial_create_goes_to_create_not_complete() {
    let mut app = app_on(MilestoneWizardStep::Materializing, chain_plan());
    wizard(&mut app).finish_materialization(Ok(MilestoneCreationResult::Partial {
        milestone_number: 9,
        issue_numbers: vec![31],
        not_created: vec!["feat: api".into(), "docs: guide".into()],
        error: "create_issue 'feat: api' failed: HTTP 502".into(),
    }));
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
