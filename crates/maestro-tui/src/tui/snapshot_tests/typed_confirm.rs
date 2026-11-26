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
/// Returns the drawn screen so a test can assert on it before the snapshot.
fn screen(app: &mut App) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    app.show_activity_log = false;
    draw(app, 80, 24)
}

#[tokio::test]
async fn typed_confirm_kill() {
    let mut app = log_app(TuiMode::Overview);
    app.pool
        .enqueue(make_session(SessionStatus::Running, Some(407)));
    let _ = handle_key(
        &mut app,
        // #1280: the kill key is `X`.
        KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE),
    )
    .await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    type_str(&mut app, "40").await;
    let terminal = screen(&mut app);
    let out = format!("{}", terminal.backend());
    assert!(out.contains("Stops the agent now"), "consequences:\n{out}");
    assert!(
        out.contains("⏎  kill (locked)"),
        "⏎ locked at 2 of 3:\n{out}"
    );
    insta::assert_snapshot!("typed_confirm_kill", terminal.backend());
}

/// Regression for the `Shift+M` bug: on the full key path the global
/// `M` (session-complete modal toggle) took the `M` of `CONFIRM`, so the
/// warning could never be confirmed.
#[tokio::test]
async fn typed_confirm_bypass() {
    let mut app = log_app(TuiMode::Overview);
    let modal_before = app.config.as_ref().map(|c| c.tui.modal_on_complete);
    let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_b).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    type_str(&mut app, "CONFIRM").await;
    assert_eq!(
        app.config.as_ref().map(|c| c.tui.modal_on_complete),
        modal_before,
        "typed M toggled the global session-complete modal"
    );
    let terminal = screen(&mut app);
    let out = format!("{}", terminal.backend());
    assert!(out.contains("› CONFIRM") && out.contains("7 of 7"), "{out}");
    assert!(
        !out.contains("(locked)"),
        "⏎ unlocked once CONFIRM is typed:\n{out}"
    );
    insta::assert_snapshot!("typed_confirm_bypass", terminal.backend());
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let _ = handle_key(&mut app, enter).await;
    assert!(app.bypass_active, "typed CONFIRM + Enter enables bypass");
    assert_ne!(app.tui_mode, TuiMode::BypassWarning, "the warning closes");
}

/// #1399: the counter counts the typed text that matches the token, so a
/// wrong letter never reads complete.
#[tokio::test]
async fn typed_confirm_counter_counts_matching_chars() {
    let mut app = log_app(TuiMode::Overview);
    let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_b).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    type_str(&mut app, "CONF").await;
    let out = format!("{}", screen(&mut app).backend());
    assert!(out.contains("4 of 7"), "matching prefix:\n{out}");
    type_str(&mut app, "IRX").await;
    let out = format!("{}", screen(&mut app).backend());
    assert!(!out.contains("7 of 7"), "CONFIRX reads complete:\n{out}");
    assert!(out.contains("does not match"), "{out}");
}

/// #1456: a session with uncommitted files in its worktree. Before `⏎`
/// the dialog says the worktree and branch go and the files are lost.
#[tokio::test]
async fn kill_confirm_lost_row() {
    let mut app = log_app(TuiMode::Overview);
    let session = make_session(SessionStatus::Running, Some(407));
    let id = session.id;
    app.pool.enqueue(session);
    let _ = app.pool.try_promote();
    let worktree = tempfile::tempdir().unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(worktree.path())
        .status()
        .expect("git must be on PATH");
    assert!(init.success());
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(worktree.path().join(name), "edit").unwrap();
    }
    let managed = app.pool.get_active_mut(id).expect("promoted");
    managed.worktree_path = Some(worktree.path().to_path_buf());
    // The promote line carries a wall-clock stamp: keep the frame stable.
    managed.session.activity_log.clear();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE),
    )
    .await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    let terminal = screen(&mut app);
    let out = format!("{}", terminal.backend());
    assert!(
        out.contains("lost   3 uncommitted files, the turn in progress"),
        "the lost row before confirm:\n{out}"
    );
    insta::assert_snapshot!("kill_confirm_lost_row", terminal.backend());
}
