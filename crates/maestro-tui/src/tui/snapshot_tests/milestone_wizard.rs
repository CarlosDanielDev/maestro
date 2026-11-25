use super::*;
use crate::adapt::types::{
    AdaptPlan, CreatedIssue, CreatedMilestone, MaterializeResult, PlannedIssue, PlannedMilestone,
};
use crate::provider::types::ProviderKind;
use crate::tui::screens::Screen;
use crate::tui::screens::adapt::types::AdaptResults;
use crate::tui::screens::adapt::{AdaptScreen, AdaptStep};
use crate::tui::screens::milestone_wizard::{
    AiGeneratedPlan, AiProposedIssue, MilestoneCreationResult, MilestoneWizardScreen,
    MilestoneWizardStep,
};
use crate::tui::theme::Theme;
use insta::assert_snapshot;

fn sample_plan() -> AiGeneratedPlan {
    AiGeneratedPlan {
        milestone_title: "M0: Foundation".into(),
        milestone_description: "Set up the core workflow.".into(),
        issues: vec![],
    }
}

fn draw_wizard_goal(kind: ProviderKind) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = MilestoneWizardScreen::with_provider_kind(kind);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    terminal
}

fn draw_wizard_complete(kind: ProviderKind) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = MilestoneWizardScreen::with_provider_kind(kind);
    screen.apply_planning_result(Ok(sample_plan()));
    screen.finish_materialization(Ok(MilestoneCreationResult::Created {
        milestone_number: 7,
        issue_numbers: vec![101, 102],
    }));

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    terminal
}

fn draw_wizard_ai_structuring_loading(
    kind: ProviderKind,
) -> Result<ratatui::Terminal<ratatui::backend::TestBackend>, Box<dyn std::error::Error>> {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = MilestoneWizardScreen::with_provider_kind(kind);
    screen.set_step_for_tests(MilestoneWizardStep::AiStructuring);
    screen.set_spinner_context(3, true);
    screen.start_planning();

    terminal.draw(|f| {
        screen.draw(f, f.area(), &theme);
    })?;

    Ok(terminal)
}

fn adapt_screen(kind: ProviderKind) -> AdaptScreen {
    let mut screen = AdaptScreen::with_provider_kind(kind);
    // `with_provider_kind` reads `.maestro/adapt-cache.json` to restore
    // resumable state. That made this fixture non-deterministic — local dev
    // hit a populated cache while CI's Coverage Tiers job runs tests
    // single-threaded on a fresh checkout with no cache. Reset to a clean
    // baseline so the snapshot reflects only what this fixture sets.
    screen.results = AdaptResults::default();
    screen.step = AdaptStep::Complete;
    screen.results.plan = Some(AdaptPlan {
        milestones: vec![PlannedMilestone {
            title: "M0: Foundation".into(),
            description: "Set up the core workflow.".into(),
            issues: vec![PlannedIssue {
                title: "feat: bootstrap".into(),
                body: "Create the first implementation slice.".into(),
                labels: vec!["enhancement".into()],
                blocked_by_titles: vec![],
            }],
        }],
        maestro_toml_patch: None,
        workflow_guide: None,
    });
    screen.results.materialize = Some(MaterializeResult {
        milestones_created: vec![CreatedMilestone {
            number: 7,
            title: "M0: Foundation".into(),
            reused: false,
        }],
        issues_created: vec![CreatedIssue {
            number: 101,
            title: "feat: bootstrap".into(),
            milestone_number: Some(7),
        }],
        issues_skipped: vec![],
        tech_debt_issue: None,
        dry_run: false,
    });
    screen
}

fn draw_adapt_complete(kind: ProviderKind) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = adapt_screen(kind);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    terminal
}

#[test]
fn milestone_wizard_goal_github() {
    let terminal = draw_wizard_goal(ProviderKind::Github);
    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_wizard_goal_azdo() {
    let terminal = draw_wizard_goal(ProviderKind::AzureDevops);
    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_wizard_complete_github() {
    let terminal = draw_wizard_complete(ProviderKind::Github);
    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_wizard_complete_azdo() {
    let terminal = draw_wizard_complete(ProviderKind::AzureDevops);
    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_wizard_ai_structuring_loading_uses_braille_spinner()
-> Result<(), Box<dyn std::error::Error>> {
    let terminal = draw_wizard_ai_structuring_loading(ProviderKind::Github)?;
    assert_snapshot!(terminal.backend());
    Ok(())
}

/// #1284: P0 schema ← P1 api, P2 guide; P0 deselected, so P1 is stranded.
fn stranded_plan() -> AiGeneratedPlan {
    let issue = |title: &str, blocked_by: Vec<usize>, accepted: bool| AiProposedIssue {
        title: title.into(),
        overview: format!("{title} overview"),
        blocked_by,
        accepted,
    };
    AiGeneratedPlan {
        milestone_title: "M1: API".into(),
        milestone_description: "Ship the API.".into(),
        issues: vec![
            issue("feat: schema", vec![], false),
            issue("feat: api", vec![0], true),
            issue("docs: guide", vec![], true),
        ],
    }
}

fn draw_screen(
    screen: &mut MilestoneWizardScreen,
) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();
    terminal
}

/// #1284 Done when: the Plan phase renders (stranded row, error on the border).
#[test]
fn milestone_wizard_plan() {
    let mut screen = MilestoneWizardScreen::with_provider_kind(ProviderKind::Github);
    screen.apply_planning_result(Ok(stranded_plan()));
    screen.set_step_for_tests(MilestoneWizardStep::ReviewPlan);
    assert!(!screen.try_advance(), "stranded P1 blocks advancing");
    assert_snapshot!(draw_screen(&mut screen).backend());
}

/// #1284 Done when: the Create phase renders, armed with the issue count.
#[test]
fn milestone_wizard_create() {
    let mut plan = stranded_plan();
    plan.issues[0].accepted = true;
    let mut screen = MilestoneWizardScreen::with_provider_kind(ProviderKind::Github);
    screen.apply_planning_result(Ok(plan));
    screen.set_step_for_tests(MilestoneWizardStep::Preview);
    screen.set_clock_for_test(std::time::Instant::now());
    screen.handle_input(
        &crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
        crate::tui::navigation::InputMode::Normal,
    );
    assert_snapshot!(draw_screen(&mut screen).backend());
}

#[test]
fn adapt_complete_github() {
    let terminal = draw_adapt_complete(ProviderKind::Github);
    assert_snapshot!(terminal.backend());
}

#[test]
fn adapt_complete_azdo() {
    let terminal = draw_adapt_complete(ProviderKind::AzureDevops);
    assert_snapshot!(terminal.backend());
}
