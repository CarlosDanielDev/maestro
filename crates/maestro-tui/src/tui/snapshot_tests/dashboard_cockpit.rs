//! #1280: the Dashboard cockpit through `ui::draw` at the three design
//! widths: Work list (Sessions · PR pipeline · Finished) with the selected
//! session's detail, and the Activity pane under it.

use std::collections::VecDeque;

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::{make_activity, make_session};
use crate::provider::github::types::{PendingPr, awaiting_pending_pr};
use crate::session::types::{GateResultEntry, SessionStatus};
use crate::tui::activity_log::ActivityLog;
use crate::tui::app::{App, TuiMode};
use crate::tui::{make_test_app, ui};

fn pending(issue: u64, errors: &[&str]) -> PendingPr {
    let mut p = awaiting_pending_pr(issue);
    p.files_touched = vec!["src/a.rs".into(), "src/b.rs".into()];
    p.cost_usd = 0.84;
    p.last_errors = errors
        .iter()
        .map(|e| e.to_string())
        .collect::<VecDeque<_>>();
    p
}

fn cockpit_app() -> App {
    let mut app = make_test_app("dashboard-cockpit");
    app.show_mascot = false;
    app.show_activity_log = true;
    app.activity_log = ActivityLog::new(100);
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));

    let mut failed = make_session(SessionStatus::FailedGates, Some(412));
    failed.id = Uuid::from_u128(412);
    failed.issue_title = Some("Refactor token refresh in auth module".into());
    failed.worktree_path = Some(".maestro/worktrees/issue-412".into());
    failed.gate_results = vec![
        GateResultEntry::pass("fmt", "cargo fmt --check"),
        GateResultEntry::pass("clippy", "0 warnings"),
        GateResultEntry::fail("test", "3 failing"),
    ];
    failed.token_usage.input_tokens = 412_000;
    failed.token_usage.output_tokens = 38_000;
    failed.activity_log = vec![
        make_activity(10, "Read src/auth/token.rs"),
        make_activity(40, "Edit src/auth/token.rs  +42 -18"),
        make_activity(60, "test failed: auth::token_refresh"),
    ];
    let mut running = make_session(SessionStatus::Running, Some(407));
    running.id = Uuid::from_u128(407);
    running.issue_title = Some("Route new sessions by 5h quota headroom".into());
    running.model = "claude-sonnet-4-5".into();
    let mut done = make_session(SessionStatus::Completed, Some(397));
    done.id = Uuid::from_u128(397);
    done.issue_title = Some("tab-bar".into());
    for s in [failed, running, done] {
        app.pool.enqueue(s);
    }

    app.pending_prs = vec![
        pending(545, &["gh: HTTP 502 Bad Gateway (createPullRequest)"]),
        pending(543, &[]),
    ];
    app.ci_poller.ci_check_details.insert(540, vec![]);
    app.tui_mode = TuiMode::Dashboard;
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t
}

#[test]
fn dashboard_wide() {
    let mut app = cockpit_app();
    assert_snapshot!(draw(&mut app, 160, 48).backend());
}

#[test]
fn dashboard_regular() {
    let mut app = cockpit_app();
    assert_snapshot!(draw(&mut app, 120, 36).backend());
}

#[test]
fn dashboard_compact() {
    let mut app = cockpit_app();
    assert_snapshot!(draw(&mut app, 80, 24).backend());
}
