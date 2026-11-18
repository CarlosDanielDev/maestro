//! `ds::confirm::TypedConfirm` drawn by the real app (#1261): the kill
//! dialog over the Dashboard, and the bypass warning.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::log_drawer::{draw, log_app};
use super::make_session;
use crate::session::types::SessionStatus;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;

async fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        let _ = handle_key(app, key).await;
    }
}

/// The activity log is hidden: the dialog would cut its clock stamps.
fn snapshot(name: &str, app: &mut App) {
    app.show_activity_log = false;
    let terminal = draw(app, 80, 24);
    insta::assert_snapshot!(name, terminal.backend());
}

#[tokio::test]
async fn typed_confirm_kill() {
    let mut app = log_app(TuiMode::Overview);
    app.pool
        .enqueue(make_session(SessionStatus::Running, Some(407)));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
    )
    .await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    type_str(&mut app, "40").await;
    snapshot("typed_confirm_kill", &mut app);
}

#[tokio::test]
async fn typed_confirm_bypass() {
    let mut app = log_app(TuiMode::Overview);
    let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_b).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    type_str(&mut app, "CONFIRM").await;
    snapshot("typed_confirm_bypass", &mut app);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let _ = handle_key(&mut app, enter).await;
    assert!(app.bypass_active, "typed CONFIRM + Enter enables bypass");
}
