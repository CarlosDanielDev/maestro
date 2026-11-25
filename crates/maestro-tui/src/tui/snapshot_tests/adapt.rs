//! §02.11 Adapt (#1281): Configure with the Stop after radio and the live
//! "This run will" plan, and Review before any write. Every screen goes
//! through `handle_key` + `ui::draw`, at 80x24 and at Wide (168x42).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;

use super::log_drawer::{draw, log_app};
use crate::adapt::types::{AdaptPlan, PlannedIssue, PlannedMilestone};
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::adapt::{AdaptResults, AdaptScreen, AdaptStep, StopAfter};

/// The Adapt screen with a clean state: `with_provider_kind` may load a
/// cache from the CWD, and the snapshot shows only what this fixture sets.
fn adapt_app() -> App {
    let mut app = log_app(TuiMode::Dashboard);
    app.navigate_to(TuiMode::AdaptWizard);
    let screen = app
        .screen_state
        .adapt_screen
        .get_or_insert_with(AdaptScreen::new);
    screen.results = AdaptResults::default();
    screen.loaded_from_cache = false;
    screen.config.path = ".".into();
    app
}

fn screen_mut(app: &mut App) -> &mut AdaptScreen {
    app.screen_state
        .adapt_screen
        .as_mut()
        .expect("adapt screen")
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn text(app: &mut App, w: u16, h: u16) -> String {
    format!("{}", draw(app, w, h).backend())
}

fn plan() -> AdaptPlan {
    let issue = |title: &str, label: &str, deps: &[&str]| PlannedIssue {
        title: title.into(),
        body: "b".into(),
        labels: vec![label.into()],
        blocked_by_titles: deps.iter().map(|d| d.to_string()).collect(),
    };
    AdaptPlan {
        milestones: vec![PlannedMilestone {
            title: "M0: Foundation".into(),
            description: "d".into(),
            issues: vec![
                issue("feat: setup", "enhancement", &[]),
                issue("test: add tests", "testing", &["feat: setup"]),
            ],
        }],
        maestro_toml_patch: None,
        workflow_guide: None,
    }
}

fn review_app() -> App {
    let mut app = adapt_app();
    let s = screen_mut(&mut app);
    StopAfter::Full.apply(&mut s.config);
    s.step = AdaptStep::Review;
    s.results.plan = Some(plan());
    app
}

#[test]
fn adapt_configure_wide() {
    let mut app = adapt_app();
    assert_eq!(app.tui_mode, TuiMode::AdaptWizard);
    let t = draw(&mut app, 168, 42);
    let shown = format!("{}", t.backend());
    assert!(shown.contains("This run will"), "the live plan is drawn");
    assert!(
        shown.contains("Plan · dry run"),
        "the default option is named"
    );
    assert_snapshot!(t.backend());
}

#[test]
fn adapt_configure_80x24() {
    let mut app = adapt_app();
    let t = draw(&mut app, 80, 24);
    let shown = format!("{}", t.backend());
    for want in [
        "Project:",
        "( ) Scan",
        "( ) Analysis",
        "(•) Plan · dry run",
        "( ) Full run",
        "Model:",
        "This run will",
        "CREATE ON REMOTE",
    ] {
        assert!(shown.contains(want), "missing {want:?} at 80x24:\n{shown}");
    }
    assert_snapshot!(t.backend());
}

#[tokio::test]
async fn configure_plan_follows_the_radio() {
    let mut app = adapt_app();
    let dry = text(&mut app, 120, 34);
    press(&mut app, KeyCode::Down).await;
    press(&mut app, KeyCode::Right).await;
    assert_eq!(screen_mut(&mut app).config.stop_after, StopAfter::Full);
    let full = text(&mut app, 120, 34);
    assert!(dry.contains("CREATE ON REMOTE") && !dry.contains("labels, milestones"));
    assert!(
        full.contains("labels, milestones, issues"),
        "Full run lists the GitHub writes"
    );
}

#[test]
fn adapt_review_wide() {
    let mut app = review_app();
    let t = draw(&mut app, 168, 42);
    let shown = format!("{}", t.backend());
    assert!(shown.contains("nothing written yet"), "the writes ledger");
    assert!(shown.contains("feat: setup"), "the plan tree");
    assert_snapshot!(t.backend());
}

#[test]
fn adapt_review_80x24() {
    let mut app = review_app();
    let t = draw(&mut app, 80, 24);
    let shown = format!("{}", t.backend());
    assert!(shown.contains("feat: setup"), "the plan tree:\n{shown}");
    assert!(shown.contains("READY"), "the armed bar:\n{shown}");
    assert_snapshot!(t.backend());
}

/// Review ⏎ ⏎ through the real dispatch: the first arms and queues
/// nothing, the second queues `RunAdaptScaffold` (the first write).
#[tokio::test]
async fn review_enter_enter_dispatches_the_scaffold() {
    let mut app = review_app();
    {
        let s = screen_mut(&mut app);
        s.results.profile = Some(crate::tui::screens::adapt::run_tests::profile(
            std::path::Path::new("."),
        ));
        s.results.report = Some(crate::adapt::types::AdaptReport {
            summary: "s".into(),
            modules: vec![],
            tech_debt_items: vec![],
        });
    }
    app.pending_commands.clear();

    press(&mut app, KeyCode::Enter).await;
    assert!(app.pending_commands.is_empty(), "the first ⏎ only arms");
    assert_eq!(screen_mut(&mut app).step, AdaptStep::Review);

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(screen_mut(&mut app).step, AdaptStep::Scaffolding);
    assert!(
        matches!(
            app.pending_commands.as_slice(),
            [TuiCommand::RunAdaptScaffold(..)]
        ),
        "the second ⏎ queues exactly the scaffold"
    );
}

const ADAPT_WRITING: &str = "An Adapt run is still writing. Exit anyway?";

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// An Adapt write in flight is live work: ⌃Q and ⌃C open Confirm Exit
/// (naming the Adapt write) instead of quitting mid-write.
#[tokio::test]
async fn adapt_write_in_flight_counts_as_live_work() {
    use crate::tui::input_handler::KeyAction;
    for step in [AdaptStep::Scaffolding, AdaptStep::Materializing] {
        for chord in [ctrl('q'), ctrl('c')] {
            let mut app = adapt_app();
            app.screen_state.quit_hints.ctrl_q_shown = true;
            screen_mut(&mut app).step = step;
            let action = handle_key(&mut app, chord).await;
            assert!(
                matches!(action, KeyAction::Consumed),
                "{step:?} {chord:?} quit mid-write"
            );
            assert!(app.running, "{step:?} {chord:?}");
            assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "{step:?} {chord:?}");
            let out = text(&mut app, 80, 24);
            assert!(out.contains(ADAPT_WRITING), "{step:?} {chord:?}:\n{out}");
        }
    }
}

/// Review writes nothing yet: ⌃Q quits at once there.
#[tokio::test]
async fn adapt_review_is_not_live_work() {
    use crate::tui::input_handler::KeyAction;
    let mut app = review_app();
    app.screen_state.quit_hints.ctrl_q_shown = true;
    let action = handle_key(&mut app, ctrl('q')).await;
    assert!(
        matches!(action, KeyAction::Quit),
        "nothing is being written"
    );
}

/// A create that fails after Review was scrolled: Failed opens at its top,
/// so "Failed during" and the error show at 80x24, and j/k scroll it.
#[tokio::test]
async fn failed_after_a_scrolled_review_shows_the_error() {
    use crate::adapt::materializer::PartialMaterialize;
    use crate::adapt::types::MaterializeResult;
    use crate::tui::app::TuiDataEvent;
    let mut app = review_app();
    for _ in 0..5 {
        press(&mut app, KeyCode::Char('j')).await;
    }
    assert_eq!(
        screen_mut(&mut app).scroll_offset,
        5,
        "setup: Review scrolled"
    );
    screen_mut(&mut app).step = AdaptStep::Materializing;
    let err = anyhow::Error::new(PartialMaterialize {
        written: MaterializeResult {
            milestones_created: vec![],
            issues_created: vec![],
            issues_skipped: vec![],
            tech_debt_issue: None,
            dry_run: false,
        },
        source: anyhow::anyhow!("HTTP 502 from GitHub"),
    });

    app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Err(err)));

    let out = text(&mut app, 80, 24);
    assert!(out.contains("Failed during"), "the phase line:\n{out}");
    assert!(out.contains("HTTP 502 from GitHub"), "the error:\n{out}");
    press(&mut app, KeyCode::Char('j')).await;
    assert_eq!(screen_mut(&mut app).scroll_offset, 1, "j scrolls Failed");
    press(&mut app, KeyCode::Char('k')).await;
    assert_eq!(screen_mut(&mut app).scroll_offset, 0, "k scrolls back");
}
