//! #1552: when startup cleanup kept branches that may hold work, the user
//! sees one line on screen (a warn toast at 80x24). Nothing when none was
//! kept.

use super::confirm_exit::{app, key, text};
use crate::session::cleanup::KeptBranch;
use crate::setup::report_kept_branches;
use crate::tui::app::TuiMode;
use crate::tui::input_handler::handle_key;

const SHOWN: &str = "[clean] Kept 2 branches that may hold work: see git branch";

fn kept(name: &str, unique_commits: Option<usize>) -> KeptBranch {
    KeptBranch {
        name: name.to_string(),
        unique_commits,
    }
}

#[tokio::test]
async fn startup_kept_branches_are_shown() {
    let mut app = app(TuiMode::Landing, 0);
    report_kept_branches(
        &mut app,
        &[
            kept("maestro/issue-7", Some(2)),
            kept("maestro/issue-8", None),
        ],
    );

    let first = text(&mut app, 80, 24);
    assert_eq!(first.matches(SHOWN).count(), 1, "one whole line:\n{first}");

    let _ = handle_key(&mut app, key('2')).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
    let after = text(&mut app, 80, 24);
    assert!(after.contains("may hold work"), "still shown:\n{after}");

    // With no log pane on screen the same line arrives as a warn toast.
    let mut app = app_without_log_pane();
    report_kept_branches(&mut app, &[kept("maestro/issue-7", Some(1))]);
    let _ = handle_key(&mut app, key('2')).await;
    let toast = text(&mut app, 80, 24);
    let line = "[clean] Kept 1 branch that may hold work: see git branch";
    assert_eq!(toast.matches(line).count(), 1, "warn toast:\n{toast}");
    assert!(toast.contains("warn"), "warn toast:\n{toast}");
}

fn app_without_log_pane() -> crate::tui::app::App {
    let mut app = app(TuiMode::Landing, 0);
    app.show_activity_log = false;
    app
}

#[tokio::test]
async fn startup_without_kept_branches_shows_nothing() {
    let mut app = app(TuiMode::Landing, 0);
    let entries = app.activity_log.entries().len();
    report_kept_branches(&mut app, &[]);
    assert_eq!(app.activity_log.entries().len(), entries, "no log line");

    let first = text(&mut app, 80, 24);
    assert!(!first.contains("may hold work"), "nothing shown:\n{first}");
    let _ = handle_key(&mut app, key('2')).await;
    let after = text(&mut app, 80, 24);
    assert!(!after.contains("may hold work"), "nothing shown:\n{after}");
}
