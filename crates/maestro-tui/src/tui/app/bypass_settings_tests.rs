//! #1239: the Settings › Sessions bypass toggle goes through the same
//! typed-CONFIRM warning as ⌃B. Keys go through the real `handle_key`
//! path and the screen is drawn at 100x30 (above the 80x24 SizeGuard).

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screen_dispatch::handle_screen_action;
use crate::tui::screens::bypass_dispatch;
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

const WARNING_MARK: &str = "DANGER — BYPASS MODE";

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

/// Types CONFIRM straight into the warning's dispatcher. Not through
/// `handle_key`: there the global Shift+M chord eats the `M` because
/// BypassWarning is not a text-input mode yet (same for ⌃B; #1261 owns
/// that allowlist).
fn type_confirm(app: &mut App) {
    for c in "CONFIRM".chars() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT));
        let action = bypass_dispatch::dispatch_input(app, &event);
        handle_screen_action(app, action);
    }
}

fn drawn(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
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

    type_confirm(&mut app);

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
    type_confirm(&mut app);

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
