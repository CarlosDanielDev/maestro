//! #1257: the status bar as `ui::draw` shows it, fed from a real App.

use crate::tui::app::TuiDataEvent;
use crate::tui::snapshot_tests::test_terminal;
use crate::tui::{make_test_app, ui};
use crate::updater::ReleaseInfo;

fn app(name: &str) -> crate::tui::app::App {
    let mut app = make_test_app(name);
    app.show_mascot = false;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.no_splash = true;
    app
}

fn screen(app: &mut crate::tui::app::App) -> String {
    let mut t = test_terminal();
    t.draw(|f| ui::draw(f, app)).unwrap();
    format!("{}", t.backend())
}

fn top_row(app: &mut crate::tui::app::App) -> String {
    screen(app).lines().next().unwrap_or_default().to_string()
}

#[test]
fn status_bar_untrusted_follows_app() {
    let mut app = app("status-bar-untrusted");
    app.workspace_trusted = false;
    let row = top_row(&mut app);
    assert!(
        row.contains("UNTRUSTED"),
        "untrusted run must say so: {row}"
    );

    app.workspace_trusted = true;
    let row = top_row(&mut app);
    assert!(
        !row.contains("UNTRUSTED"),
        "trusted run shows no chip: {row}"
    );
}

#[test]
fn status_bar_shows_bypass_chip_from_app() {
    let mut app = app("status-bar-bypass");
    app.bypass_active = true;
    let out = screen(&mut app);
    let row = out.lines().next().unwrap_or_default();
    assert!(row.contains("▲ BYPASS"), "{row}");
    assert_eq!(out.matches("BYPASS").count(), 1, "one bypass chip: {out}");
}

#[test]
fn update_notice_single_place() {
    let mut app = app("status-bar-update");
    app.show_activity_log = true;
    app.handle_data_event(TuiDataEvent::VersionCheckResult(Some(ReleaseInfo {
        tag: "v9.8.7".to_string(),
        version: "9.8.7".to_string(),
        download_url: "https://github.com/CarlosDanielDev/maestro/releases/download/v9.8.7/x"
            .to_string(),
    })));
    let out = screen(&mut app);
    assert_eq!(
        out.matches("9.8.7").count(),
        1,
        "the update notice must appear exactly once: {out}"
    );
    let row = out.lines().next().unwrap_or_default();
    assert!(row.contains("↑ 9.8.7 u"), "the one place is the bar: {row}");
}
