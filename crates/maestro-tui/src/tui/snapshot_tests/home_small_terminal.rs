//! Snapshot tests for the home screen on small terminals (#1157, #1253).
//!
//! At 60x15 (the size #1157 pinned) the terminal is below the 80x24 floor, so
//! `ui::draw` shows the SizeGuard instead of an overlapping home. At 80x24,
//! the smallest size still laid out, the home keeps what #1157 pinned: Quick
//! Actions labels are never cut mid-word without an `…`, and the activity log
//! keeps at least one content row below the home panes.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::activity_log::{LogEntry, LogLevel};
use crate::tui::app::TuiMode;
use crate::tui::screens::home::{HomeScreen, ProjectInfo, SessionSummary, Suggestion};
use crate::tui::snapshot_tests::fixed_start;
use crate::tui::{make_test_app, ui};

const LOG_MESSAGE: &str = "home-log-row-1157";

fn small_home_app() -> crate::tui::app::App {
    let mut app = make_test_app("home-small-terminal");
    app.show_mascot = false;
    app.show_activity_log = true;
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

    let recent = vec![SessionSummary {
        issue_number: 2,
        title: "Start the work".to_string(),
        status: "completed".to_string(),
        cost_usd: 0.5,
    }];
    let mut screen = HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some("carlos".to_string()),
        },
        recent,
        vec![],
    );
    screen.set_suggestions(vec![Suggestion {
        kind: crate::tui::screens::home::SuggestionKind::ReadyIssues { count: 3 },
        message: "3 issues labeled ready".to_string(),
        action: TuiMode::IssueBrowser,
    }]);
    app.screen_state.home_screen = Some(screen);
    app.tui_mode = TuiMode::Dashboard;

    app.activity_log.push(LogEntry {
        timestamp: local_ten_am(),
        session_label: "S-1".to_string(),
        message: LOG_MESSAGE.to_string(),
        level: LogLevel::Info,
        tool_meta: None,
    });
    app
}

/// `ui::draw` renders the activity log in the machine's zone and has no
/// zone seam, so anchor the entry at 10:00:00 local time on
/// [`fixed_start`]'s date: the snapshot reads the same under any `TZ`.
fn local_ten_am() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&fixed_start().date_naive().and_hms_opt(10, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .with_timezone(&chrono::Utc)
}

#[test]
fn home_screen_60x15_shows_the_size_guard() {
    let mut app = small_home_app();
    let mut t = Terminal::new(TestBackend::new(60, 15)).unwrap();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let output = format!("{}", t.backend());
    assert!(
        output.contains("Terminal too small") && output.contains("60 x 15"),
        "60x15 is below the 80x24 floor and must show the SizeGuard:\n{output}"
    );
    assert!(
        !output.contains("Quick Actions"),
        "no home layout below the floor:\n{output}"
    );
    assert_snapshot!(t.backend());
}

#[test]
fn home_screen_80x24_keeps_labels_and_a_log_row() {
    let mut app = small_home_app();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let output = format!("{}", t.backend());
    // #1280: the Dashboard is the Work list now; its empty state is whole.
    assert!(
        output.contains("Nothing running · start work from 1 Home"),
        "the empty Work list says what to do:\n{output}"
    );
    assert!(
        output.contains(LOG_MESSAGE),
        "activity log must keep at least one content row:\n{output}"
    );
    assert_snapshot!(t.backend());
}
