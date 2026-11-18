//! #1239: the Settings › Sessions bypass toggle goes through the same
//! typed-CONFIRM warning as ⌃B, with its own scope text. Every key,
//! CONFIRM and ⏎ included, goes through the real `handle_key` path, and the
//! screen is drawn at 80x24 or larger (the SizeGuard minimum).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::settings::{SettingsScreen, SettingsTab};
use crate::tui::{make_test_app, ui};

const SETTINGS_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nascii_icons = false\n",
);

/// In both titles: the ⌃B one and the Settings one.
const WARNING_MARK: &str = "enable bypass";

fn settings_app() -> App {
    let mut app = make_test_app("settings-bypass");
    app.bypass_active = false;
    app.bypass_warning_acknowledged = false;
    let config = toml::from_str(SETTINGS_TOML).expect("config parses");
    let mut screen = SettingsScreen::new(config, app.flags.clone());
    screen.jump_to_tab(SettingsTab::Sessions);
    app.screen_state.settings_screen = Some(screen);
    app.tui_mode = TuiMode::Settings;
    app
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

/// Focus the bypass row (idx 3 of the Sessions tab) and press Space.
async fn toggle_bypass_row(app: &mut App) {
    for _ in 0..3 {
        press(app, KeyCode::Down).await;
    }
    press(app, KeyCode::Char(' ')).await;
}

/// Types CONFIRM (Shift letters, as a terminal sends them), then ⏎,
/// through the full `handle_key` path.
async fn type_confirm(app: &mut App) {
    for c in "CONFIRM".chars() {
        handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)).await;
    }
    press(app, KeyCode::Enter).await;
}

fn drawn(app: &mut App) -> String {
    drawn_at(app, 100, 30)
}

fn drawn_at(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).expect("terminal");
    t.draw(|f| ui::draw(f, app)).expect("draw");
    let buf = t.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn staged_mode(app: &App) -> String {
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    screen.config.sessions.permission_mode.clone()
}

#[tokio::test]
async fn settings_bypass_toggle_requires_confirm() {
    let mut app = settings_app();
    let before = staged_mode(&app);
    assert_ne!(before, "bypassPermissions", "setup: bypass starts off");

    toggle_bypass_row(&mut app).await;

    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    assert!(
        drawn(&mut app).contains(WARNING_MARK),
        "warning must be drawn"
    );
    assert_eq!(staged_mode(&app), before, "not staged before CONFIRM");
    assert!(!app.bypass_active);

    type_confirm(&mut app).await;

    assert_eq!(app.tui_mode, TuiMode::Settings);
    assert!(!drawn(&mut app).contains(WARNING_MARK));
    assert_eq!(
        staged_mode(&app),
        "bypassPermissions",
        "staged after CONFIRM"
    );
    assert!(app.bypass_active);
}

#[tokio::test]
async fn settings_bypass_cancel_reverts() {
    let mut app = settings_app();
    let before = staged_mode(&app);

    toggle_bypass_row(&mut app).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    press(&mut app, KeyCode::Esc).await;

    assert_eq!(app.tui_mode, TuiMode::Settings);
    assert!(!drawn(&mut app).contains(WARNING_MARK));
    assert_eq!(staged_mode(&app), before, "permission_mode unchanged");
    assert!(!app.bypass_active);
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    assert!(!screen.bypass_request_pending(), "the toggle is back off");

    // The next key in Settings must not reopen the warning.
    press(&mut app, KeyCode::Up).await;
    assert_eq!(app.tui_mode, TuiMode::Settings);
    assert!(!drawn(&mut app).contains(WARNING_MARK));
}

#[tokio::test]
async fn settings_bypass_logs_source() {
    let mut app = settings_app();

    toggle_bypass_row(&mut app).await;
    type_confirm(&mut app).await;

    let last_bypass = app
        .activity_log
        .entries()
        .iter()
        .rev()
        .find(|e| e.message.starts_with("Bypass mode enabled"))
        .expect("a bypass log entry");
    assert!(
        last_bypass.message.contains("settings (typed CONFIRM)"),
        "source must be settings (typed CONFIRM), got: {}",
        last_bypass.message
    );
}

/// A global chord can leave the warning without answering it. The chord is
/// simulated with `navigate_to`, so the test does not depend on which
/// letters the warning takes as text (#1261).
fn leave_warning_unanswered(app: &mut App) {
    app.navigate_to(TuiMode::Landing);
}

#[tokio::test]
async fn settings_bypass_stale_request_not_inherited_by_ctrl_b() {
    let mut app = settings_app();
    let before = staged_mode(&app);
    toggle_bypass_row(&mut app).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    leave_warning_unanswered(&mut app);

    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
    )
    .await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    assert!(drawn(&mut app).contains(WARNING_MARK));
    type_confirm(&mut app).await;

    let last_bypass = (app.activity_log.entries().iter().rev())
        .find(|e| e.message.starts_with("Bypass mode enabled"))
        .expect("a bypass log entry");
    assert!(
        last_bypass.message.contains("(tui)"),
        "got: {}",
        last_bypass.message
    );
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    assert!(
        !screen.bypass_request_pending(),
        "Settings toggle is back off"
    );
    assert_eq!(staged_mode(&app), before, "⌃B stages nothing in Settings");
}

#[tokio::test]
async fn settings_bypass_stale_request_reverts_on_return() {
    let mut app = settings_app();
    let before = staged_mode(&app);
    toggle_bypass_row(&mut app).await;
    leave_warning_unanswered(&mut app);

    app.tui_mode = TuiMode::Settings;
    press(&mut app, KeyCode::Down).await;

    assert_eq!(app.tui_mode, TuiMode::Settings);
    assert!(app.screen_state.bypass_warning_screen.is_none());
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    assert!(
        !screen.bypass_request_pending(),
        "Settings toggle is back off"
    );
    assert_eq!(staged_mode(&app), before);
    assert!(!drawn(&mut app).contains(WARNING_MARK));
}

#[tokio::test]
async fn ctrl_b_keeps_a_saved_settings_bypass_row_on() {
    let mut app = settings_app();
    let screen = app.screen_state.settings_screen.as_mut().expect("settings");
    screen.config.sessions.permission_mode = "bypassPermissions".into();
    toggle_bypass_row(&mut app).await;
    app.tui_mode = TuiMode::Landing;
    assert_eq!(staged_mode(&app), "bypassPermissions", "setup: staged on");

    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
    )
    .await;
    press(&mut app, KeyCode::Esc).await;

    let screen = app.screen_state.settings_screen.as_mut().expect("settings");
    screen.sync_widgets_to_config();
    assert_eq!(
        staged_mode(&app),
        "bypassPermissions",
        "⌃B must not touch it"
    );
}

#[tokio::test]
async fn settings_bypass_warning_states_the_repo_scope() {
    let mut app = settings_app();
    toggle_bypass_row(&mut app).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);

    let screen = drawn_at(&mut app, 80, 24);
    assert!(screen.contains("maestro.toml (committed)"), "{screen}");
    assert!(screen.contains("every future session"), "{screen}");
    assert!(!screen.contains("THIS session only"), "{screen}");
}

#[tokio::test]
async fn ctrl_b_warning_keeps_the_session_scope() {
    let mut app = settings_app();
    app.tui_mode = TuiMode::Landing;
    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
    )
    .await;

    let screen = drawn_at(&mut app, 80, 24);
    assert!(screen.contains("THIS session only"), "{screen}");
    assert!(!screen.contains("maestro.toml (committed)"), "{screen}");
}
