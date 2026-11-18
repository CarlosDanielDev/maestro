//! #1260 (judge of PR #1410): "idle" means no live work, not only an empty
//! pool. A queue between items and a completion / auto-PR job still
//! running keep `q` and ⌃Q behind Confirm Exit. Exit chords ignore key
//! release events, and ⌃Q quits from the idle dialog too.

use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::confirm_exit::{app, ctrl, key, text};
use crate::provider::types::Issue;
use crate::state::types::PendingIssueCompletion;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::{KeyAction, handle_key};
use crate::work::dependencies::DependencyGraph;
use crate::work::executor::QueueExecutor;
use crate::work::queue::WorkQueue;
use crate::work::types::WorkItem;

const LIVE_WORK: &str = "A queue or PR job is still running. Exit anyway?";

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
        assert!(out.contains(LIVE_WORK), "{what}:\n{out}");
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
