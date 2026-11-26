//! #1455: two selected issues with equivalent titles block Create. The
//! provider reuses an issue by title, so one of them would be lost
//! silently. Key tests go through `handle_key` + `ui::draw` at 80x24;
//! helpers are shared with `create_tests.rs`. No real GitHub writes: the
//! create is only queued, and `materialize_plan_with_client` runs against
//! `MockGitHubClient`.

use super::super::{
    AiGeneratedPlan, MilestoneCreationResult, MilestoneWizardScreen, MilestoneWizardStep,
    materialize_plan_with_client,
};
use super::{DUPLICATE_REASON, duplicates};
use super::tests::{app_on, chain_plan, create_queued, issue, press, screen_text, wizard};
use crossterm::event::KeyCode;
use std::time::{Duration, Instant};

/// Done when: "duplicate titles in one plan block Create with a reason on
/// the Plan step".
#[tokio::test]
async fn duplicate_titles_block_create() {
    let mut plan = chain_plan();
    plan.issues[2].title = " Feat:  Schema ".into(); // equivalent to P0
    let mut app = app_on(MilestoneWizardStep::ReviewPlan, plan);
    assert_eq!(wizard(&mut app).validation_error(), Some(DUPLICATE_REASON));
    assert_eq!(wizard(&mut app).review_focus(), 0);

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(
        wizard(&mut app).step(),
        MilestoneWizardStep::ReviewPlan,
        "⏎ never reaches Create while two selected issues share a title"
    );
    assert_eq!(
        wizard(&mut app).review_focus(),
        2,
        "⏎ focuses the repeated row, so x drops the marked one"
    );
    let screen = screen_text(&mut app);
    assert!(
        screen.contains("\"feat: schema\" is planned twice · drop one with x"),
        "the border names the repeated title:\n{screen}"
    );
    let marked: Vec<&str> = screen.lines().filter(|l| l.contains("[!]")).collect();
    assert_eq!(marked.len(), 1, "only the repeat is marked:\n{screen}");
    assert!(
        marked[0].contains("Feat:  Schema") && marked[0].contains("planned twice"),
        "the repeated row is marked:\n{screen}"
    );

    // x acts on the focused (marked) row; that clears the block.
    press(&mut app, KeyCode::Char('x')).await;
    assert_eq!(wizard(&mut app).validation_error(), None);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    assert!(!create_queued(&app), "advancing to Create writes nothing");
    assert_eq!(
        wizard(&mut app).create_description(),
        "creates 1 milestone + 2 issues"
    );
}

/// A deselected issue is not written, so its title never blocks.
#[test]
fn a_deselected_duplicate_does_not_block() {
    let mut plan = chain_plan();
    plan.issues[2].title = "feat: schema".into();
    plan.issues[2].accepted = false;
    assert!(duplicates(&plan.issues).is_empty());
    let mut s = MilestoneWizardScreen::new();
    s.apply_planning_result(Ok(plan));
    s.set_step_for_tests(MilestoneWizardStep::ReviewPlan);
    assert_eq!(s.validation_error(), None);
}

#[test]
fn duplicates_pairs_each_repeat_with_the_first() {
    let issues = vec![
        issue("a", vec![]),
        issue("b", vec![]),
        issue(" A ", vec![]),
        issue("a", vec![]),
    ];
    assert_eq!(duplicates(&issues), vec![(2, 0), (3, 0)]);
}

/// Done when: "with distinct titles, N planned issues create N issues on
/// the mock client". A regression guard: the duplicate check must not
/// block or drop near-equal but distinct titles. Plan → Create → ⏎ ⏎
/// through `handle_key`, then the queued plan runs on the mock.
#[tokio::test]
async fn distinct_titles_create_all() {
    use crate::provider::github::client::mock::MockGitHubClient;

    let plan = AiGeneratedPlan {
        milestone_title: "M3: Distinct".into(),
        milestone_description: "d".into(),
        issues: vec![
            issue("feat: api", vec![]),
            issue("feat: api v2", vec![0]),
            issue("docs: api", vec![]),
            issue("feat: api client", vec![1]),
        ],
        written: None,
    };
    let mut app = app_on(MilestoneWizardStep::ReviewPlan, plan);
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).step(), MilestoneWizardStep::Preview);
    let screen = screen_text(&mut app);
    assert!(!screen.contains("planned twice"), "no false mark:\n{screen}");
    press(&mut app, KeyCode::Enter).await;
    wizard(&mut app).set_clock_for_test(t0 + Duration::from_secs(1));
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&mut app).materialize_progress(), Some((0, 4)));

    let queued = wizard(&mut app).generated_plan().cloned().expect("plan");
    let client = MockGitHubClient::new();
    let result = materialize_plan_with_client(&queued, &client)
        .await
        .expect("create");
    assert_eq!(client.create_issue_calls().len(), 4, "one create per issue");
    let MilestoneCreationResult::Created { issue_numbers, .. } = result else {
        panic!("expected Created, got {result:?}");
    };
    let mut unique = issue_numbers.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 4, "4 distinct issues: {issue_numbers:?}");
}
