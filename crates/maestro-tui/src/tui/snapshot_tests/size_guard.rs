//! `ui::draw` below 80x24 draws the SizeGuard screen for every mode (#1253).

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use crate::tui::app::TuiMode;
use crate::tui::{make_test_app, ui};

/// Every `TuiMode`. The `match` has no wildcard, so a new mode fails to
/// compile here until it is added to the list.
fn all_modes() -> Vec<TuiMode> {
    let id = Uuid::nil();
    let modes = vec![
        TuiMode::Overview,
        TuiMode::Detail(id),
        TuiMode::DependencyGraph,
        TuiMode::AgentGraph,
        TuiMode::Fullscreen(id),
        TuiMode::CostDashboard,
        TuiMode::Dashboard,
        TuiMode::Landing,
        TuiMode::IssueBrowser,
        TuiMode::MilestoneView,
        TuiMode::PromptInput,
        TuiMode::Interaction,
        TuiMode::CompletionSummary,
        TuiMode::ContinuousPause,
        TuiMode::QueueConfirmation,
        TuiMode::QueueExecution,
        TuiMode::HollowRetry,
        TuiMode::TokenDashboard,
        TuiMode::Sanitize,
        TuiMode::Settings,
        TuiMode::SessionSwitcher,
        TuiMode::AdaptWizard,
        TuiMode::PrReview,
        TuiMode::ReleaseNotes,
        TuiMode::IssueWizard,
        TuiMode::MilestoneWizard,
        TuiMode::ProjectStats,
        TuiMode::LogViewer(id),
        TuiMode::ConfirmKill(id),
        TuiMode::ConfirmExit,
        TuiMode::SessionSummary,
        TuiMode::AdaptFollowUp,
        TuiMode::Prd,
        TuiMode::Roadmap,
        TuiMode::BypassWarning,
        TuiMode::MilestoneHealth,
        TuiMode::GateOutputViewer(id),
        TuiMode::CiErrorReview,
        TuiMode::TeamWizard,
        TuiMode::BudgetPreSpawn { session_id: id },
        TuiMode::CallLog(id),
        TuiMode::AdoptPicker,
        TuiMode::FleetBoard,
    ];
    for mode in &modes {
        match mode {
            TuiMode::Overview
            | TuiMode::Detail(_)
            | TuiMode::DependencyGraph
            | TuiMode::AgentGraph
            | TuiMode::Fullscreen(_)
            | TuiMode::CostDashboard
            | TuiMode::Dashboard
            | TuiMode::Landing
            | TuiMode::IssueBrowser
            | TuiMode::MilestoneView
            | TuiMode::PromptInput
            | TuiMode::Interaction
            | TuiMode::CompletionSummary
            | TuiMode::ContinuousPause
            | TuiMode::QueueConfirmation
            | TuiMode::QueueExecution
            | TuiMode::HollowRetry
            | TuiMode::TokenDashboard
            | TuiMode::Sanitize
            | TuiMode::Settings
            | TuiMode::SessionSwitcher
            | TuiMode::AdaptWizard
            | TuiMode::PrReview
            | TuiMode::ReleaseNotes
            | TuiMode::IssueWizard
            | TuiMode::MilestoneWizard
            | TuiMode::ProjectStats
            | TuiMode::LogViewer(_)
            | TuiMode::ConfirmKill(_)
            | TuiMode::ConfirmExit
            | TuiMode::SessionSummary
            | TuiMode::AdaptFollowUp
            | TuiMode::Prd
            | TuiMode::Roadmap
            | TuiMode::BypassWarning
            | TuiMode::MilestoneHealth
            | TuiMode::GateOutputViewer(_)
            | TuiMode::CiErrorReview
            | TuiMode::TeamWizard
            | TuiMode::BudgetPreSpawn { .. }
            | TuiMode::CallLog(_)
            | TuiMode::AdoptPicker
            | TuiMode::FleetBoard => {}
        }
    }
    modes
}

fn draw_mode(mode: TuiMode, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut app = make_test_app("size-guard");
    app.tui_mode = mode;
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    terminal
}

#[test]
fn size_guard_79x24() {
    let terminal = draw_mode(TuiMode::Dashboard, 79, 24);
    assert_snapshot!(terminal.backend());
}

#[test]
fn every_mode_draws_the_size_guard_below_80x24() {
    for (w, h) in [(79, 24), (80, 23)] {
        for mode in all_modes() {
            let out = format!("{}", draw_mode(mode, w, h).backend());
            assert!(
                out.contains("Terminal too small") && out.contains("80 x 24"),
                "{mode:?} at {w}x{h} did not draw the SizeGuard:\n{out}"
            );
            assert!(
                out.contains(&format!("{w} x {h}")),
                "{mode:?} at {w}x{h} must show the current size:\n{out}"
            );
        }
    }
}

#[test]
fn size_guard_does_not_replace_a_minimum_size_terminal() {
    let out = format!("{}", draw_mode(TuiMode::Dashboard, 80, 24).backend());
    assert!(!out.contains("Terminal too small"), "{out}");
}

#[test]
fn ui_draw_does_not_panic_on_a_1x1_terminal() {
    draw_mode(TuiMode::Dashboard, 1, 1);
}

/// A confirm that the SizeGuard hid would still take `y` and act unseen, so
/// every confirm draws on top of the guard.
#[test]
fn confirm_exit_stays_visible_over_the_size_guard_at_60x15() {
    let terminal = draw_mode(TuiMode::ConfirmExit, 60, 15);
    let out = format!("{}", terminal.backend());
    assert!(out.contains("Terminal too small"), "{out}");
    assert!(
        out.contains("Are you sure you want to exit?") && out.contains("[y]es"),
        "the quit confirm must stay visible below 80x24:\n{out}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn other_confirms_stay_visible_over_the_size_guard_at_60x15() {
    let id = Uuid::nil();
    let cases = [
        (TuiMode::ConfirmKill(id), "Kill session"),
        (TuiMode::BudgetPreSpawn { session_id: id }, "[y]es"),
    ];
    for (mode, text) in cases {
        let out = format!("{}", draw_mode(mode, 60, 15).backend());
        assert!(out.contains(text), "{mode:?} confirm hidden:\n{out}");
    }
}
