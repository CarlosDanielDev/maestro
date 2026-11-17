//! Full-app Landing tests (#1226): `ui::draw` in `TuiMode::Landing`.
//!
//! The activity log is 3 rows (border + one content row) on the landing when
//! the terminal is shorter than 40 rows. From 40 rows it keeps today's height.
//! `make_test_app` has `config: None`, so that height is the fallback of 10
//! rows, not 25% of the terminal.

use ratatui::{Terminal, backend::TestBackend};

use crate::tui::activity_log::{LogEntry, LogLevel};
use crate::tui::app::{App, TuiMode};
use crate::tui::screens::landing::LandingScreen;
use crate::tui::snapshot_tests::fixed_start;
use crate::tui::{make_test_app, ui};

const LOG_MESSAGE: &str = "landing-log-row-1226";
/// `ui.rs` fallback when `app.config` is `None`.
const FALLBACK_LOG_ROWS: usize = 10;

fn landing_app() -> App {
    let mut app = make_test_app("landing-full-app");
    app.show_mascot = false;
    app.show_activity_log = true;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));
    app.screen_state.landing_screen = Some(LandingScreen::new());
    app.tui_mode = TuiMode::Landing;
    app.activity_log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-1".to_string(),
        message: LOG_MESSAGE.to_string(),
        level: LogLevel::Info,
        tool_meta: None,
    });
    app
}

fn draw_rows(app: &mut App, w: u16, h: u16) -> Vec<String> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn row_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("missing {needle:?}:\n{}", rows.join("\n")))
}

#[test]
fn landing_log_is_three_rows_below_40_rows() {
    for h in [24u16, 27, 39] {
        let mut app = landing_app();
        let rows = draw_rows(&mut app, 80, h);
        let h = h as usize;
        assert_eq!(row_of(&rows, "Activity Log"), h - 4, "80x{h}: title row");
        assert_eq!(row_of(&rows, LOG_MESSAGE), h - 3, "80x{h}: content row");
    }
}

#[test]
fn landing_log_keeps_the_old_height_from_40_rows() {
    for (w, h) in [(80u16, 40u16), (160, 50)] {
        let mut app = landing_app();
        let rows = draw_rows(&mut app, w, h);
        let h = h as usize;
        let title = h - 1 - FALLBACK_LOG_ROWS;
        assert_eq!(row_of(&rows, "Activity Log"), title, "{w}x{h}: title row");
        assert_eq!(
            row_of(&rows, LOG_MESSAGE),
            title + 1,
            "{w}x{h}: content row"
        );
    }
}

#[test]
fn landing_80x27_shows_the_compact_console_above_the_log() {
    let mut app = landing_app();
    let rows = draw_rows(&mut app, 80, 27);
    let text = rows.join("\n");
    for needle in [
        "Release Console v",
        "┌Mix",
        "┌Highlights",
        "<q> quit",
        LOG_MESSAGE,
    ] {
        assert!(text.contains(needle), "missing {needle:?}:\n{text}");
    }
    // Status bar is row 0 (#1257); the compact header sits below one pad row.
    assert!(
        rows[2].contains("MAESTRO  v"),
        "row 2: {:?}\n{text}",
        rows[2]
    );
    assert!(!text.contains("|  \\/  |"), "big logo must be off:\n{text}");
}

#[test]
fn landing_80x24_is_menu_only_with_a_three_row_log() {
    let mut app = landing_app();
    let rows = draw_rows(&mut app, 80, 24);
    let text = rows.join("\n");
    assert!(!text.contains("Release Console v"), "{text}");
    assert_eq!(row_of(&rows, "Activity Log"), 20);
    assert_eq!(row_of(&rows, LOG_MESSAGE), 21);
}

#[test]
fn confirm_exit_over_landing_keeps_the_three_row_log() {
    let mut app = landing_app();
    app.nav_stack.push(TuiMode::Landing);
    app.tui_mode = TuiMode::ConfirmExit;
    let rows = draw_rows(&mut app, 80, 27);
    assert_eq!(row_of(&rows, "Activity Log"), 23);
    assert_eq!(row_of(&rows, LOG_MESSAGE), 24);
}

#[test]
fn dashboard_log_height_is_unchanged_at_80x30() {
    let mut app = landing_app();
    app.tui_mode = TuiMode::Dashboard;
    let rows = draw_rows(&mut app, 80, 30);
    assert_eq!(row_of(&rows, "Activity Log"), 30 - 1 - FALLBACK_LOG_ROWS);
}

#[test]
fn landing_without_the_activity_log_still_shows_the_console() {
    let mut app = landing_app();
    app.show_activity_log = false;
    let rows = draw_rows(&mut app, 80, 27);
    let text = rows.join("\n");
    assert!(!text.contains("Activity Log"), "{text}");
    assert!(text.contains("Release Console v"), "{text}");
}
