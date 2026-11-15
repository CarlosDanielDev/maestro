//! #1158: a session that finishes while "Exit anyway?" is open must not
//! steal the dialog's focus, so the user's `y` still quits.

use super::input_handler::{KeyAction, handle_key};
use super::{app::TuiMode, completion_ready, make_test_app};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// App on Overview with one running session.
fn app_with_running_session() -> (super::app::App, uuid::Uuid) {
    let mut app = make_test_app("confirm-exit-race-1158");
    app.tui_mode = TuiMode::Overview;
    let session = crate::session::types::Session::new(
        "prompt".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    let id = session.id;
    app.pool.enqueue(session);
    let _ = app.pool.try_promote();
    (app, id)
}

/// The event-loop tail that opens the summary once every session is done.
fn deliver_completion(app: &mut super::app::App, id: uuid::Uuid) {
    app.pool.finalize_retain_worktree(id);
    if completion_ready(app, true) {
        app.open_completion_summary();
    }
}

#[tokio::test]
async fn y_after_completion_under_confirm_exit_quits() {
    let (mut app, id) = app_with_running_session();
    handle_key(&mut app, key('q')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);

    deliver_completion(&mut app, id);
    let action = handle_key(&mut app, key('y')).await;

    assert!(matches!(action, KeyAction::Quit), "`y` must quit");
    assert!(!app.running);
}

#[tokio::test]
async fn completion_under_confirm_exit_keeps_dialog_focused() {
    let (mut app, id) = app_with_running_session();
    handle_key(&mut app, key('q')).await;

    deliver_completion(&mut app, id);

    assert_eq!(
        app.tui_mode,
        TuiMode::ConfirmExit,
        "completion must not change which screen gets the next key"
    );
    assert!(app.completion_summary.is_some(), "summary is still built");

    // Cancelling the dialog lands on the summary, then back to Overview.
    handle_key(&mut app, key('n')).await;
    assert_eq!(app.tui_mode, TuiMode::CompletionSummary);
    assert!(app.running);
    assert_eq!(app.nav_stack.peek(), Some(&TuiMode::Overview));
}
