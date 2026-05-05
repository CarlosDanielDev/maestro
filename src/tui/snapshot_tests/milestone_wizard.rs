use super::*;
use crate::adapt::types::{
    AdaptPlan, CreatedIssue, CreatedMilestone, MaterializeResult, PlannedIssue, PlannedMilestone,
};
use crate::provider::types::ProviderKind;
use crate::tui::screens::Screen;
use crate::tui::screens::adapt::{AdaptScreen, AdaptStep};
use crate::tui::screens::milestone_wizard::{
    AiGeneratedPlan, MilestoneCreationResult, MilestoneWizardScreen, MilestoneWizardStep,
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
