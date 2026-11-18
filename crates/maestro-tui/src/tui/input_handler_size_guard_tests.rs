//! Keys under the SizeGuard (#1253). At 60x15 the screen is hidden, so only a
//! confirm drawn on top of the guard takes keys, plus `Ctrl+C`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use super::{KeyAction, handle_key, handle_key_under_size_guard};
use crate::session::discovery::DiscoveredSession;
use crate::tui::app::{App, TuiMode};
use crate::tui::navigation::InputMode;
use crate::tui::screens::Screen;
use crate::tui::screens::fleet_board::FleetBoardScreen;
use crate::tui::screens::fleet_board::rows::build_rows;
use crate::tui::screens::queue_confirmation::QueueConfirmationScreen;
use crate::tui::screens::settings::{SettingsScreen, SettingsTab};
use crate::tui::screens::test_helpers::key_event;
use crate::work::conflicts::{ConflictReport, IssueWithFiles};

const SMALL: Rect = Rect::new(0, 0, 60, 15);

/// The run loop's order: the guard first, then the normal dispatcher.
async fn press_at(app: &mut App, key: KeyEvent, area: Rect) -> KeyAction {
    match handle_key_under_size_guard(app, key, area).await {
        Some(action) => action,
        None => handle_key(app, key).await,
    }
}

async fn press(app: &mut App, code: KeyCode) -> KeyAction {
    press_at(app, KeyEvent::new(code, KeyModifiers::NONE), SMALL).await
}

fn app() -> App {
    crate::tui::make_test_app("size-guard-keys")
}

const SETTINGS_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nascii_icons = false\n",
);

#[tokio::test]
async fn settings_discard_is_not_confirmed_under_the_guard() {
    let mut app = app();
    let config = toml::from_str(SETTINGS_TOML).expect("config parses");
    let mut screen = SettingsScreen::new(config, app.flags.clone());
    screen.jump_to_tab(SettingsTab::Notifications);
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.is_dirty(), "setup: unsaved edit");
    app.screen_state.settings_screen = Some(screen);
    app.tui_mode = TuiMode::Settings;

    press(&mut app, KeyCode::Esc).await;
    press(&mut app, KeyCode::Char('y')).await;

    assert_eq!(app.tui_mode, TuiMode::Settings, "Esc, y must not leave");
    let screen = app.screen_state.settings_screen.as_ref().unwrap();
    assert!(screen.is_dirty(), "the unsaved edit must survive");
}

#[tokio::test]
async fn fleet_live_confirm_does_not_adopt_under_the_guard() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let live = DiscoveredSession {
        session_id: "ext-size-guard".to_string(),
        cwd: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        title: "still typing".to_string(),
        last_active: now - Duration::from_secs(30),
        skipped_lines: 0,
    };
    let mut board = FleetBoardScreen::new(build_rows(&[], "maestro", &[live]), now);
    board.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert!(board.confirm_pending(), "setup: live confirm open");
    let mut app = app();
    app.screen_state.fleet_board_screen = Some(board);
    app.tui_mode = TuiMode::FleetBoard;

    press(&mut app, KeyCode::Char('y')).await;

    assert_eq!(app.tui_mode, TuiMode::FleetBoard, "y must not adopt");
    assert!(
        app.pool.all_sessions().is_empty(),
        "no session may be adopted"
    );
    let board = app.screen_state.fleet_board_screen.as_ref().unwrap();
    assert!(
        board.confirm_pending(),
        "the confirm stays armed, untouched"
    );
}

#[tokio::test]
async fn queue_confirmation_enter_does_not_launch_under_the_guard() {
    let issues = vec![IssueWithFiles {
        issue_number: 7,
        files_to_modify: Some(vec!["src/seven.rs".to_string()]),
    }];
    let titles = HashMap::from([(7, "Issue #7".to_string())]);
    let report = ConflictReport {
        conflicts: vec![],
        unknown_scope_issues: vec![],
        is_safe: true,
    };
    let mut app = app();
    app.screen_state.queue_confirmation_screen =
        Some(QueueConfirmationScreen::new(issues, &titles, report));
    app.tui_mode = TuiMode::QueueConfirmation;

    press(&mut app, KeyCode::Enter).await;

    assert_eq!(
        app.tui_mode,
        TuiMode::QueueConfirmation,
        "Enter must not launch"
    );
    assert!(app.screen_state.queue_confirmation_screen.is_some());
}

#[tokio::test]
async fn confirm_exit_drawn_on_the_guard_still_quits_on_y() {
    let mut app = app();
    app.tui_mode = TuiMode::ConfirmExit;
    let action = press(&mut app, KeyCode::Char('y')).await;
    assert!(matches!(action, KeyAction::Quit));
    assert!(!app.running);
}

/// #1260: Ctrl+C opens Confirm Exit (drawn on the guard), then `y` quits.
#[tokio::test]
async fn ctrl_c_under_the_guard_opens_confirm_exit() {
    let mut app = app();
    app.tui_mode = TuiMode::Dashboard;
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = press_at(&mut app, ctrl_c, SMALL).await;
    assert!(matches!(action, KeyAction::Consumed));
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    let action = press(&mut app, KeyCode::Char('y')).await;
    assert!(matches!(action, KeyAction::Quit));
}

#[tokio::test]
async fn global_chords_are_swallowed_under_the_guard() {
    let chords = [
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
    ];
    for mode in [TuiMode::Dashboard, TuiMode::ConfirmExit] {
        for chord in chords {
            // #1260: inside an idle Confirm Exit, ⌃Q is the quit key (like `y`).
            let ctrl_q =
                chord.code == KeyCode::Char('q') && chord.modifiers.contains(KeyModifiers::CONTROL);
            if mode == TuiMode::ConfirmExit && ctrl_q {
                continue;
            }
            let mut app = app();
            app.tui_mode = mode;
            let flags_before = format!("{:?}", app.flags);
            let action = press_at(&mut app, chord, SMALL).await;
            assert!(matches!(action, KeyAction::Consumed), "{mode:?} {chord:?}");
            assert!(app.running, "{mode:?} {chord:?} must not quit");
            assert_eq!(app.tui_mode, mode, "{mode:?} {chord:?} changed the mode");
            assert_eq!(format!("{:?}", app.flags), flags_before, "{chord:?}");
            assert!(!app.bypass_active, "{chord:?} toggled bypass");
        }
    }
}

#[tokio::test]
async fn idle_confirm_exit_on_the_guard_quits_on_ctrl_q() {
    let mut app = app();
    app.tui_mode = TuiMode::ConfirmExit;
    let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
    let action = press_at(&mut app, ctrl_q, SMALL).await;
    assert!(matches!(action, KeyAction::Quit));
}

#[tokio::test]
async fn keys_take_the_normal_path_at_80x24() {
    let mut app = app();
    app.tui_mode = TuiMode::Dashboard;
    let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(
        handle_key_under_size_guard(&mut app, q, Rect::new(0, 0, 80, 24))
            .await
            .is_none()
    );
}
