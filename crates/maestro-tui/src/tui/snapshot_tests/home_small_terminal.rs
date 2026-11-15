//! Snapshot test for the home screen on a 60x15 terminal (#1157).
//!
//! Pins two things a tiny terminal used to break: Quick Actions labels are
//! never cut mid-word without an `…`, and the activity log keeps at least one
//! content row below the home panes.

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
fn home_screen_60x15_keeps_labels_and_a_log_row() {
    let mut app = small_home_app();
    let mut t = Terminal::new(TestBackend::new(60, 15)).unwrap();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let output = format!("{}", t.backend());
    assert!(
        output.contains("Browse Issues") || output.contains('…'),
        "Quick Actions labels must be whole or end in `…`:\n{output}"
    );
    assert!(
        !output.contains("Browse Is║"),
        "label clipped mid-word without `…`:\n{output}"
    );
    assert!(
        output.contains(LOG_MESSAGE),
        "activity log must keep at least one content row:\n{output}"
    );
    assert_snapshot!(t.backend());
}
