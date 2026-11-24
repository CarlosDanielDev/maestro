//! #1244 — the queue executor must know which session runs its current
//! item, read the outcome from that session (not from whatever session
//! entered the pool last), treat a hollow completion as a failure, and
//! keep each queued issue's body so `Blocked-by` is honoured.

#![cfg(test)]

use crate::provider::types::Issue;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, TuiMode};
use crate::tui::screens::{ScreenAction, SessionConfig};
use crate::work::dependencies::DependencyGraph;
use crate::work::executor::{ExecutorPhase, QueueExecutor, QueueItemState};
use crate::work::queue::WorkQueue;
use crate::work::types::WorkItem;
use ratatui::{Terminal, backend::TestBackend};

fn make_app() -> App {
    crate::tui::make_test_app("maestro-queue-1244")
}

fn issue(number: u64, body: &str) -> Issue {
    Issue {
        number,
        title: format!("Issue #{number}"),
        body: body.to_string(),
        labels: vec![],
        state: "open".to_string(),
        html_url: String::new(),
        milestone: None,
        assignees: vec![],
    }
}

/// An executor over `issues`, already advanced onto the first item.
fn running_executor(issues: &[u64]) -> QueueExecutor {
    let items: Vec<WorkItem> = issues
        .iter()
        .map(|&n| WorkItem::from_issue(issue(n, "")))
        .collect();
    let queue = WorkQueue::validate_selection(issues, &DependencyGraph::build(&items))
        .expect("independent test queue");
    let mut exec = QueueExecutor::new(&queue);
    exec.advance().expect("advance from idle");
    exec
}

fn session_for(issue_number: Option<u64>) -> Session {
    Session::new(
        "work".into(),
        "opus".into(),
        "orchestrator".into(),
        issue_number,
        None,
    )
}

/// Put `session` in the pool's finished list with `status`.
fn finish(app: &mut App, mut session: Session, status: SessionStatus, hollow: bool) {
    session.status = status;
    session.is_hollow_completion = hollow;
    let id = session.id;
    app.pool.enqueue(session);
    for promoted in app.pool.try_promote() {
        if promoted == id {
            app.pool.finalize_and_teardown(id);
        }
    }
}

/// Fill every pool slot so `add_session` enqueues without spawning a
/// provider process.
fn fill_slots(app: &mut App) {
    for _ in 0..3 {
        app.pool.enqueue(session_for(None));
    }
    app.pool.try_promote();
}

fn draw(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
    t.draw(|f| crate::tui::ui::draw(f, app)).expect("draw");
    format!("{}", t.backend())
}

#[tokio::test]
async fn queue_spawn_records_session_id() {
    let mut app = make_app();
    fill_slots(&mut app);
    app.queue_executor = Some(running_executor(&[5, 6]));

    // A session for another issue must not be tied to the item.
    let _ = app.add_session(session_for(Some(9))).await;
    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(exec.items()[0].session_id, None);

    let session = session_for(Some(5));
    let spawned = session.id;
    let _ = app.add_session(session).await;

    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(exec.items()[0].session_id, Some(spawned));
    assert_eq!(exec.items()[1].session_id, None);
}

#[test]
fn hollow_completion_is_not_success() {
    let mut app = make_app();
    let mut exec = running_executor(&[5, 6]);
    let hollow = session_for(Some(5));
    exec.set_session_id(hollow.id);
    app.queue_executor = Some(exec);
    finish(&mut app, hollow, SessionStatus::Completed, true);
    // An unrelated session that succeeded after it must not decide the
    // item's outcome.
    finish(&mut app, session_for(None), SessionStatus::Completed, false);

    app.settle_queue_item();

    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(exec.items()[0].state, QueueItemState::Failed);
    assert_eq!(exec.items()[1].state, QueueItemState::Pending);
    assert_eq!(
        exec.phase(),
        ExecutorPhase::AwaitingDecision { failed_index: 0 }
    );

    app.tui_mode = TuiMode::QueueExecution;
    let screen = draw(&mut app);
    // #1287: the run loop's decision card replaced the overlay.
    assert!(
        screen.contains("#5 failed") && screen.contains("hollow"),
        "queue screen must show the hollow item as failed:\n{screen}"
    );
    assert!(
        screen.contains("Retry #5"),
        "retry decision missing:\n{screen}"
    );
    assert!(!screen.contains("running #6"), "queue advanced:\n{screen}");
}

#[test]
fn real_completion_advances_queue() {
    let mut app = make_app();
    let mut exec = running_executor(&[5, 6]);
    let done = session_for(Some(5));
    exec.set_session_id(done.id);
    app.queue_executor = Some(exec);
    finish(&mut app, done, SessionStatus::Completed, false);
    // A failed unrelated session added later must not fail the item.
    finish(&mut app, session_for(None), SessionStatus::Errored, false);

    app.settle_queue_item();

    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(exec.items()[0].state, QueueItemState::Succeeded);
    assert_eq!(exec.phase(), ExecutorPhase::Running { current_index: 1 });
}

#[test]
fn queue_waits_until_item_session_is_known() {
    // Right after advance the next issue is still being fetched: the pool
    // is idle but no session runs the item yet. That is not an outcome.
    let mut app = make_app();
    app.queue_executor = Some(running_executor(&[5, 6]));
    finish(
        &mut app,
        session_for(Some(4)),
        SessionStatus::Completed,
        false,
    );

    app.settle_queue_item();

    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(exec.items()[0].state, QueueItemState::Running);
    assert_eq!(exec.phase(), ExecutorPhase::Running { current_index: 0 });
}

#[tokio::test]
async fn hollow_queue_item_does_not_open_hollow_modal() {
    // The queue screen's retry / skip / abort is the decision for a queue
    // item. A second decision in the hollow modal would spawn a retry the
    // executor never sees.
    let mut app = make_app();
    let mut exec = running_executor(&[5, 6]);
    let hollow = session_for(Some(5));
    exec.set_session_id(hollow.id);
    app.queue_executor = Some(exec);
    finish(&mut app, hollow, SessionStatus::Completed, true);

    app.check_completions().await.expect("check_completions");

    assert!(app.screen_state.hollow_retry_screen.is_none());
}

/// The Issues browser holds the list the user multi-selects from; the
/// fetch cache is empty until an issue is launched.
fn app_with_browser(issues: Vec<Issue>) -> App {
    let mut app = make_app();
    app.state.issue_cache.clear();
    app.screen_state.issue_browser_screen =
        Some(crate::tui::screens::IssueBrowserScreen::new(issues));
    app
}

#[test]
fn launch_queue_keeps_issue_bodies() {
    let app = app_with_browser(vec![
        issue(5, "First."),
        issue(6, "Second.\n\nBlocked-by: #4"),
    ]);
    let configs = vec![
        SessionConfig {
            issue_number: Some(5),
            title: "Issue #5".into(),
            ..SessionConfig::default()
        },
        SessionConfig {
            issue_number: Some(6),
            title: "Issue #6".into(),
            ..SessionConfig::default()
        },
    ];

    let items = crate::tui::screen_dispatch::queue_work_items(&app, &configs);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].issue.body, "First.");
    assert_eq!(items[1].issue.body, "Second.\n\nBlocked-by: #4");
    assert_eq!(items[1].blocked_by, vec![4]);
}

#[test]
fn launch_queue_refuses_dependent_selection() {
    // #6 is blocked by #5 and both are selected. Launching them all at once
    // would run #6 before its blocker is done.
    let mut app = app_with_browser(vec![
        issue(5, "First."),
        issue(6, "Second.\n\nBlocked-by: #5"),
    ]);
    app.tui_mode = TuiMode::QueueConfirmation;
    let configs = vec![
        SessionConfig {
            issue_number: Some(5),
            ..SessionConfig::default()
        },
        SessionConfig {
            issue_number: Some(6),
            ..SessionConfig::default()
        },
    ];

    crate::tui::screen_dispatch::handle_screen_action(&mut app, ScreenAction::LaunchQueue(configs));

    assert!(app.queue_executor.is_none());
    assert!(
        !app.pending_commands
            .iter()
            .any(|c| matches!(c, crate::tui::app::TuiCommand::LaunchSessions(_))),
        "dependent issues must not be launched in parallel"
    );
    assert_eq!(app.tui_mode, TuiMode::QueueConfirmation);
    let screen = draw(&mut app);
    assert!(
        screen.contains("Queue not started: Issue #6 depends on #5"),
        "the refusal must be visible on screen:\n{screen}"
    );
}

#[tokio::test]
async fn manual_hollow_session_on_past_queue_issue_opens_hollow_modal() {
    // #5 already succeeded in the queue. A later manual session on #5 that
    // ends hollow is not the queue's decision: the hollow modal must open.
    let mut app = make_app();
    let mut exec = running_executor(&[5, 6]);
    let queued = session_for(Some(5));
    exec.set_session_id(queued.id);
    exec.mark_success();
    app.queue_executor = Some(exec);
    finish(&mut app, queued, SessionStatus::Completed, false);
    finish(
        &mut app,
        session_for(Some(5)),
        SessionStatus::Completed,
        true,
    );

    app.check_completions().await.expect("check_completions");

    assert!(app.screen_state.hollow_retry_screen.is_some());
    let screen = draw(&mut app);
    assert!(
        screen.contains("finished without doing any work"),
        "hollow modal not drawn:\n{screen}"
    );
}

#[tokio::test]
async fn queue_hollow_session_stays_closed_after_queue_retry() {
    // The queue decided on the hollow session; after [r] Retry clears the
    // item's record, the old session must not pop the modal a second time.
    let mut app = make_app();
    let mut exec = running_executor(&[5, 6]);
    let hollow = session_for(Some(5));
    exec.set_session_id(hollow.id);
    app.queue_executor = Some(exec);
    finish(&mut app, hollow, SessionStatus::Completed, true);
    app.settle_queue_item();
    if let Some(exec) = app.queue_executor.as_mut() {
        exec.apply_decision(crate::work::executor::FailureAction::Retry);
        exec.advance().expect("advance after retry");
    }

    app.check_completions().await.expect("check_completions");

    assert!(app.screen_state.hollow_retry_screen.is_none());
}

#[test]
fn failed_fetch_of_running_item_offers_decision() {
    let mut app = make_app();
    app.queue_executor = Some(running_executor(&[5, 6]));
    // A failed fetch for an unrelated issue leaves the item running.
    app.handle_data_event(crate::tui::app::TuiDataEvent::Issue(
        Err(crate::tui::background_tasks::IssueFetchError::wrap(
            9,
            anyhow::anyhow!("not found"),
        )),
        None,
        None,
    ));
    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(exec.phase(), ExecutorPhase::Running { current_index: 0 });

    app.handle_data_event(crate::tui::app::TuiDataEvent::Issue(
        Err(crate::tui::background_tasks::IssueFetchError::wrap(
            5,
            anyhow::anyhow!("not found"),
        )),
        None,
        None,
    ));

    let exec = app.queue_executor.as_ref().expect("executor");
    assert_eq!(
        exec.phase(),
        ExecutorPhase::AwaitingDecision { failed_index: 0 }
    );
    app.tui_mode = TuiMode::QueueExecution;
    let screen = draw(&mut app);
    assert!(
        screen.contains("#5 failed") && screen.contains("no session started"),
        "fetch failure must reach the decision card:\n{screen}"
    );
}
