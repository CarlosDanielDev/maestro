//! Upgrade keys (`u` / `n` / `Esc`) act only on a bare key and never while a
//! text field has focus (#1368). Typed text always reaches the field.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use super::{KeyAction, handle_key, handle_key_under_size_guard};
use crate::tui::app::{App, TuiMode};
use crate::tui::screens::InteractionScreen;
use crate::updater::{ReleaseInfo, UpgradeState};

/// The smallest size that is laid out: keys reach the screens.
const AREA: Rect = Rect::new(0, 0, 80, 24);

async fn send(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> KeyAction {
    let key = KeyEvent::new(code, modifiers);
    match handle_key_under_size_guard(app, key, AREA).await {
        Some(action) => action,
        None => handle_key(app, key).await,
    }
}

async fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        send(app, KeyCode::Char(c), KeyModifiers::NONE).await;
    }
}

/// The download double: a non-GitHub URL fails the installer's trust check
/// before any network call or file write, so a started download is harmless.
fn available() -> UpgradeState {
    UpgradeState::Available(ReleaseInfo {
        tag: "v99.0.0".into(),
        version: "99.0.0".into(),
        download_url: "http://127.0.0.1/untrusted-1368".into(),
    })
}

fn ready_to_restart() -> UpgradeState {
    UpgradeState::ReadyToRestart {
        version: "99.0.0".into(),
        backup_path: "/nonexistent-1368".into(),
    }
}

fn app_in_composer(state: UpgradeState) -> App {
    let mut app = crate::tui::make_test_app("upgrade-keys");
    app.tui_mode = TuiMode::Interaction;
    app.screen_state.interaction_screen = Some(InteractionScreen::new());
    app.upgrade_state = state;
    app
}

fn composer_text(app: &App) -> String {
    app.screen_state
        .interaction_screen
        .as_ref()
        .expect("interaction screen")
        .editor_text()
}

#[tokio::test]
async fn upgrade_u_ignored_in_text_input() {
    let mut app = app_in_composer(available());
    type_text(&mut app, "bug").await;
    assert_eq!(composer_text(&app), "bug");
    assert!(matches!(app.upgrade_state, UpgradeState::Available(_)));
}

#[tokio::test]
async fn upgrade_esc_reaches_screen_in_text_input() {
    let mut app = app_in_composer(available());
    send(&mut app, KeyCode::Esc, KeyModifiers::NONE).await;
    assert!(
        matches!(app.upgrade_state, UpgradeState::Available(_)),
        "Esc belongs to the text field, not the update chip"
    );
}

#[tokio::test]
async fn upgrade_ctrl_u_is_not_upgrade() {
    let mut app = app_in_composer(available());
    type_text(&mut app, "ab").await;
    send(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL).await;
    assert!(matches!(app.upgrade_state, UpgradeState::Available(_)));
    assert_ne!(composer_text(&app), "ab", "Ctrl+U is the composer undo");
}

#[tokio::test]
async fn upgrade_alt_u_is_not_upgrade_outside_text_input() {
    let mut app = crate::tui::make_test_app("upgrade-keys");
    app.tui_mode = TuiMode::Dashboard;
    app.upgrade_state = available();
    send(&mut app, KeyCode::Char('u'), KeyModifiers::ALT).await;
    send(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL).await;
    assert!(matches!(app.upgrade_state, UpgradeState::Available(_)));
}

#[tokio::test]
async fn upgrade_bare_u_outside_text_input_downloads() {
    let mut app = crate::tui::make_test_app("upgrade-keys");
    app.tui_mode = TuiMode::Dashboard;
    app.upgrade_state = available();
    send(&mut app, KeyCode::Char('u'), KeyModifiers::NONE).await;
    assert!(matches!(
        app.upgrade_state,
        UpgradeState::Downloading { ref version } if version == "99.0.0"
    ));
}

#[tokio::test]
async fn upgrade_bare_esc_outside_text_input_hides_chip() {
    let mut app = crate::tui::make_test_app("upgrade-keys");
    app.tui_mode = TuiMode::Dashboard;
    app.upgrade_state = available();
    send(&mut app, KeyCode::Esc, KeyModifiers::NONE).await;
    assert!(matches!(app.upgrade_state, UpgradeState::Hidden));
}

#[tokio::test]
async fn ready_to_restart_n_ignored_in_text_input() {
    let mut app = app_in_composer(ready_to_restart());
    type_text(&mut app, "no").await;
    assert_eq!(composer_text(&app), "no");
    assert!(matches!(
        app.upgrade_state,
        UpgradeState::ReadyToRestart { .. }
    ));
}
