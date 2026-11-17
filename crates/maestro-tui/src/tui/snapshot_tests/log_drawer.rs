//! Activity log placement (#1263): the pane lives on Home and Dashboard only.
//! Elsewhere WARN/ERR entries raise a toast and `Ctrl+L` opens the log
//! drawer, sized by `tui.layout.activity_log_height`.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::make_gh_issue;
use crate::config::Config;
use crate::tui::activity_log::LogLevel;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::toast::Severity;
use crate::tui::input_handler::handle_key;
use crate::tui::screens::IssueBrowserScreen;
use crate::tui::screens::home::{HomeScreen, ProjectInfo};
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::{make_test_app, ui};

const SEEDED: &str = "seeded log line";

const CONFIG_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nascii_icons = false\n",
);

fn config(log_height: Option<u8>) -> Config {
    let mut toml = CONFIG_TOML.to_string();
    if let Some(pct) = log_height {
        toml.push_str(&format!("[tui.layout]\nactivity_log_height = {pct}\n"));
    }
    toml::from_str(&toml).expect("config parses")
}

fn log_app(mode: TuiMode) -> App {
    let mut app = make_test_app("log-drawer");
    app.show_mascot = false;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.status_bar_marquee = crate::tui::marquee::MarqueeState::new();
    app.status_bar_marquee_fingerprint = 0;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));
    app.screen_state.home_screen = Some(HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some("carlos".to_string()),
        },
        vec![],
        vec![],
    ));
    app.screen_state.issue_browser_screen = Some(IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]));
    app.screen_state.settings_screen = Some(SettingsScreen::new(config(None), app.flags.clone()));
    app.activity_log
        .push_simple("S-1".into(), SEEDED.into(), LogLevel::Info);
    app.tui_mode = mode;
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t
}

/// Mask every `HH:MM:SS` stamp, so snapshots do not depend on the clock or
/// the machine's time zone.
fn with_time_mask(body: impl FnOnce()) {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "HH:MM:SS");
    settings.bind(body);
}

fn row_text(t: &Terminal<TestBackend>, y: u16) -> String {
    let buf = t.backend().buffer();
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn ctrl_l() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)
}

fn esc() -> KeyEvent {
    KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
}

#[test]
fn activity_pane_only_on_home_and_dashboard() {
    for mode in [TuiMode::Landing, TuiMode::Dashboard, TuiMode::Overview] {
        let mut app = log_app(mode);
        let out = format!("{}", draw(&mut app, 100, 40).backend());
        assert!(out.contains(SEEDED), "{mode:?} must show the pane:\n{out}");
    }
    for mode in [
        TuiMode::IssueBrowser,
        TuiMode::Settings,
        TuiMode::MilestoneView,
        TuiMode::Roadmap,
        TuiMode::ProjectStats,
        TuiMode::Prd,
        TuiMode::IssueWizard,
        TuiMode::Detail(uuid::Uuid::nil()),
    ] {
        let mut app = log_app(mode);
        let out = format!("{}", draw(&mut app, 100, 40).backend());
        assert!(
            !out.contains(SEEDED),
            "{mode:?} must not show the pane:\n{out}"
        );
    }
}

#[test]
fn issues_screen_has_no_activity_pane() {
    let mut app = log_app(TuiMode::IssueBrowser);
    let t = draw(&mut app, 80, 24);
    with_time_mask(|| assert_snapshot!("issues_screen_no_activity_pane", t.backend()));
}

#[test]
fn settings_screen_has_no_activity_pane() {
    let mut app = log_app(TuiMode::Settings);
    let t = draw(&mut app, 80, 24);
    with_time_mask(|| assert_snapshot!("settings_screen_no_activity_pane", t.backend()));
}

#[test]
fn warn_on_non_log_screen_raises_toast() {
    let mut app = log_app(TuiMode::IssueBrowser);
    app.activity_log
        .push_simple("GH".into(), "fetch failed".into(), LogLevel::Warn);
    draw(&mut app, 80, 24);
    let view = app.toasts.visible(Instant::now()).expect("a toast");
    assert_eq!(view.severity, Severity::Warn);
    assert!(view.message.contains("fetch failed"), "{:?}", view.message);

    // An error is a sticky error toast.
    let mut app = log_app(TuiMode::IssueBrowser);
    app.activity_log
        .push_simple("AUTH".into(), "auth lost".into(), LogLevel::Error);
    draw(&mut app, 80, 24);
    let view = app.toasts.visible(Instant::now()).expect("a toast");
    assert_eq!(view.severity, Severity::Error);

    // Info stays in the log only.
    let mut app = log_app(TuiMode::IssueBrowser);
    draw(&mut app, 80, 24);
    assert!(
        app.toasts.visible(Instant::now()).is_none(),
        "info: no toast"
    );

    // On Home the pane shows the warning; no toast, and none later either.
    let mut app = log_app(TuiMode::Landing);
    app.activity_log
        .push_simple("GH".into(), "fetch failed".into(), LogLevel::Warn);
    draw(&mut app, 100, 40);
    app.tui_mode = TuiMode::IssueBrowser;
    draw(&mut app, 80, 24);
    assert!(app.toasts.visible(Instant::now()).is_none(), "seen on Home");
}

#[tokio::test]
async fn ctrl_l_toggles_log_drawer() {
    for mode in [
        TuiMode::IssueBrowser,
        TuiMode::Settings,
        TuiMode::Landing,
        TuiMode::Dashboard,
        TuiMode::Roadmap,
        TuiMode::Interaction,
    ] {
        let mut app = log_app(mode);
        assert!(!app.activity_log.drawer_open);
        let _ = handle_key(&mut app, ctrl_l()).await;
        assert!(app.activity_log.drawer_open, "{mode:?}: Ctrl+L opens");
        let _ = handle_key(&mut app, ctrl_l()).await;
        assert!(!app.activity_log.drawer_open, "{mode:?}: Ctrl+L closes");

        let _ = handle_key(&mut app, ctrl_l()).await;
        let _ = handle_key(&mut app, esc()).await;
        assert!(!app.activity_log.drawer_open, "{mode:?}: Esc closes");
        assert_eq!(app.tui_mode, mode, "{mode:?}: Esc only closes the drawer");
    }
}

#[test]
fn log_drawer_open() {
    let mut app = log_app(TuiMode::IssueBrowser);
    app.activity_log
        .push_simple("S-2".into(), "second line".into(), LogLevel::Tool);
    app.activity_log.drawer_open = true;
    let t = draw(&mut app, 80, 24);
    let out = format!("{}", t.backend());
    assert!(out.contains(SEEDED), "drawer shows the log:\n{out}");
    with_time_mask(|| assert_snapshot!("log_drawer_open", t.backend()));
}

/// Top and bottom border rows of the drawer box.
fn drawer_rows(t: &Terminal<TestBackend>) -> (u16, u16) {
    let h = t.backend().buffer().area.height;
    let top = (0..h)
        .find(|y| row_text(t, *y).contains("Activity Log"))
        .expect("drawer title row");
    let bottom = (top + 1..h)
        .find(|y| row_text(t, *y).starts_with('╰'))
        .expect("drawer bottom border");
    (top, bottom)
}

#[test]
fn activity_log_height_sizes_drawer() {
    // 30% of 40 rows = 12 rows; 50% = 20 rows; the 10..=50 clamp still holds.
    for (pct, rows) in [(30u8, 12u16), (50, 20), (90, 20)] {
        let mut app = log_app(TuiMode::IssueBrowser);
        app.config = Some(config(Some(pct)));
        assert_eq!(
            app.config.as_ref().unwrap().tui.layout.activity_log_height,
            pct,
            "parses"
        );
        app.activity_log.drawer_open = true;
        let t = draw(&mut app, 80, 40);
        let (top, bottom) = drawer_rows(&t);
        assert_eq!(bottom - top + 1, rows, "{pct}%");
        assert_eq!(bottom, 38, "drawer sits right above the footer");
    }
}
