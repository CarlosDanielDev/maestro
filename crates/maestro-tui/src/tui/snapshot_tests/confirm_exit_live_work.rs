//! #1260 (judge of PR #1410): "idle" means no live work, not only an empty
//! pool. A queue between items and a completion / auto-PR job still
//! running keep `q` and ⌃Q behind Confirm Exit. Exit chords ignore key
//! release events, and ⌃Q quits from the idle dialog too.

use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::confirm_exit::{app, ctrl, key, text};
use crate::provider::types::Issue;
use crate::provider::types::ProviderKind;
use crate::session::team_runner::CancellationToken;
use crate::state::types::PendingIssueCompletion;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::{KeyAction, handle_key};
use crate::tui::screens::TeamWizardScreen;
use crate::work::dependencies::DependencyGraph;
use crate::work::executor::{ExecutorPhase, QueueExecutor};
use crate::work::queue::WorkQueue;
use crate::work::types::WorkItem;

const LIVE_WORK: &str = "A queue or PR job is still running. Exit anyway?";
const TEAM_RUN: &str = "A team run is between waves. Exit anyway?";
const AWAITING: &str = "A queue is waiting for your decision. Exit anyway?";

/// A two-item queue whose first item succeeded: the pool is empty and the
/// next item is not spawned yet.
fn queue_between_items() -> QueueExecutor {
    let items: Vec<WorkItem> = [1, 2]
        .iter()
        .map(|&n| {
            WorkItem::from_issue(Issue {
                number: n,
                title: format!("Issue #{n}"),
                body: String::new(),
                labels: vec![],
                state: "open".to_string(),
                html_url: String::new(),
                milestone: None,
                assignees: vec![],
            })
        })
        .collect();
    let queue = WorkQueue::validate_selection(&[1, 2], &DependencyGraph::build(&items))
        .expect("independent queue");
    let mut exec = QueueExecutor::new(&queue);
    exec.advance().expect("first item");
    exec.mark_success();
    assert!(!exec.is_finished());
    exec
}

fn completion_in_flight() -> PendingIssueCompletion {
    PendingIssueCompletion {
        issue_number: 7,
        issue_numbers: vec![],
        success: true,
        cost_usd: 0.0,
        files_touched: vec![],
        worktree_branch: None,
        worktree_path: None,
        is_ci_fix: false,
    }
}

/// `q` and ⌃Q on Home with only `setup`'s work running open Confirm Exit,
/// which names the work, and `y` does not quit.
async fn assert_asks(setup: fn(&mut App), what: &str) {
    assert_asks_with(setup, what, LIVE_WORK).await;
}

async fn assert_asks_with(setup: fn(&mut App), what: &str, question: &str) {
    for chord in [key('q'), ctrl('q')] {
        let mut app = app(TuiMode::Landing, 0);
        setup(&mut app);
        let action = handle_key(&mut app, chord).await;
        assert!(
            matches!(action, KeyAction::Consumed),
            "{what}: {chord:?} quit"
        );
        assert!(app.running, "{what}: {chord:?}");
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "{what}: {chord:?}");
        let out = text(&mut app, 80, 24);
        assert!(out.contains(question), "{what}:\n{out}");
        let action = handle_key(&mut app, key('y')).await;
        assert!(matches!(action, KeyAction::Consumed), "{what}: y quit");
    }
}

#[tokio::test]
async fn queue_between_items_counts_as_live_work() {
    assert_asks(
        |app| app.queue_executor = Some(queue_between_items()),
        "queue between items",
    )
    .await;
}

#[tokio::test]
async fn running_completion_job_counts_as_live_work() {
    assert_asks(
        |app| app.completions_in_flight.push(completion_in_flight()),
        "completion job",
    )
    .await;
}

fn team_run(app: &mut App) {
    let mut wizard = TeamWizardScreen::new(ProviderKind::default());
    wizard.set_run_cancel(CancellationToken::new());
    app.screen_state.team_wizard_screen = Some(wizard);
}

/// `run_team_until` runs detached and fetches the next wave's issue while
/// the pool is empty: the run counts until its result lands.
#[tokio::test]
async fn team_run_between_waves_counts_as_live_work() {
    assert_asks_with(team_run, "team run", TEAM_RUN).await;

    let mut app = app(TuiMode::Landing, 0);
    team_run(&mut app);
    if let Some(wizard) = app.screen_state.team_wizard_screen.as_mut() {
        wizard.apply_launch_result(Ok(()));
    }
    let action = handle_key(&mut app, key('q')).await;
    assert!(
        matches!(action, KeyAction::Quit),
        "a finished run is not live"
    );
}

/// A queue paused on a failed item is live, but nothing is running: the
/// dialog says it waits for a decision.
#[tokio::test]
async fn queue_awaiting_decision_says_waiting() {
    assert_asks_with(
        |app| {
            let mut exec = queue_between_items();
            exec.advance().expect("second item");
            exec.mark_failure();
            assert!(matches!(
                exec.phase(),
                ExecutorPhase::AwaitingDecision { .. }
            ));
            app.queue_executor = Some(exec);
        },
        "queue awaiting decision",
        AWAITING,
    )
    .await;
}

#[tokio::test]
async fn idle_confirm_exit_ctrl_q_quits() {
    for open in [ctrl('x'), ctrl('c')] {
        let mut app = app(TuiMode::Landing, 0);
        let _ = handle_key(&mut app, open).await;
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
        let action = handle_key(&mut app, ctrl('q')).await;
        assert!(matches!(action, KeyAction::Quit), "{open:?} then ⌃Q");
    }
}

fn release(c: char) -> KeyEvent {
    KeyEvent {
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
        ..KeyEvent::new(crossterm::event::KeyCode::Char(c), KeyModifiers::CONTROL)
    }
}

/// A terminal that reports releases sends one after every press: it must
/// not quit, open the dialog, or finish it.
#[tokio::test]
async fn exit_chord_release_is_ignored() {
    for c in ['q', 'c', 'x'] {
        let mut app = app(TuiMode::Landing, 0);
        let action = handle_key(&mut app, release(c)).await;
        assert!(matches!(action, KeyAction::Consumed), "release ⌃{c}");
        assert!(app.running, "release ⌃{c} quit");
        assert_eq!(app.tui_mode, TuiMode::Landing, "release ⌃{c}");
    }
    let mut app = app(TuiMode::Landing, 0);
    let _ = handle_key(&mut app, ctrl('x')).await;
    let action = handle_key(&mut app, release('q')).await;
    assert!(
        matches!(action, KeyAction::Consumed),
        "release ⌃Q in the dialog"
    );
    assert!(app.running);
}

/// #1489: a quit drops a queued upgrade (it waits for the agents that the
/// quit stops), so Confirm Exit names it.
#[tokio::test]
async fn confirm_exit_names_queued_upgrade() {
    use crate::updater::{ReleaseInfo, UpgradeState};
    for chord in [key('q'), ctrl('q')] {
        let mut app = app(TuiMode::Landing, 1);
        app.upgrade_state = UpgradeState::Queued(ReleaseInfo {
            tag: "v9.8.7".into(),
            version: "9.8.7".into(),
            download_url: "http://127.0.0.1/untrusted-1489".into(),
        });
        let _ = handle_key(&mut app, chord).await;
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "{chord:?}");
        let out = text(&mut app, 80, 24);
        assert!(
            out.contains("Upgrade to 9.8.7 is queued: it will not install if you quit."),
            "{chord:?}:\n{out}"
        );
    }
}

/// #1557: Confirm Restart's footer names the `y` hold, the ⌃Q way out
/// and Esc, and fits at 80x24.
#[tokio::test]
async fn confirm_restart_footer_80x24() {
    use crate::updater::UpgradeState;
    let mut app = app(TuiMode::Landing, 1);
    app.show_activity_log = false;
    app.upgrade_state = UpgradeState::ReadyToRestart {
        version: "9.8.7".into(),
        backup_path: "/nonexistent-1557".into(),
    };
    let _ = handle_key(&mut app, key('y')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    let out = text(&mut app, 80, 24);
    let footer = out.lines().last().unwrap_or_default();
    for hint in ["y hold to restart", "⌃Q quit", "Esc stay"] {
        assert!(footer.contains(hint), "{hint}: {footer}");
    }
    insta::assert_snapshot!("confirm_restart_footer_80x24", out);
}

/// #1560: the upgrade banner owns its area left of the pinned safety
/// chips. No status bar text shows after `[n]o`, and BYPASS stays on the
/// row next to it.
#[tokio::test]
async fn upgrade_banner_clears_status_bar_text_under_it() {
    use crate::updater::UpgradeState;
    for bypass in [false, true] {
        let mut app = app(TuiMode::Landing, 1);
        app.show_activity_log = false;
        app.bypass_active = bypass;
        app.upgrade_state = UpgradeState::ReadyToRestart {
            version: "9.8.7".into(),
            backup_path: "/nonexistent-1560".into(),
        };
        let _ = handle_key(&mut app, key('y')).await;
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "bypass {bypass}");
        let out = text(&mut app, 80, 24);
        let row = out.lines().next().unwrap_or_default();
        assert!(row.contains("[n]o"), "bypass {bypass}: {row}");
        let banner = match row.find("▲ BYPASS") {
            Some(at) => &row[..at],
            None => {
                assert!(
                    !bypass,
                    "the BYPASS chip must stay next to the banner: {row}"
                );
                row
            }
        };
        assert!(!banner.contains("spent"), "bypass {bypass}: {row}");
    }
}
