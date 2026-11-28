//! #1563: the untracked-dir notice in the real launch order of `maestro`:
//! `startup_cleanup` just before `tui::run`, then the run loop's
//! `announce_quit_key`, then the FIRST frame on Landing at 80x24, with a
//! real config (its layout, not the no-config defaults).

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::confirm_exit::text;
use super::log_drawer::log_app;
use crate::setup::orphans::SHOWN_MARKER;
use crate::setup::orphans::tests::{repo_with, worktree};
use crate::setup::report_startup_orphans;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::{announce_quit_key, handle_key};

const CONFIG: &str = r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "claude"
"#;

/// The app and its first frame for a launch in `repo`.
fn first_frame(repo: &std::path::Path) -> (App, String) {
    let mut app = log_app(TuiMode::Landing);
    app.config = Some(toml::from_str(CONFIG).expect("test config"));
    app.screen_state.quit_hints.ctrl_q_shown = false;
    // What `tui::run` sets for the `maestro` boot path.
    app.screen_state.landing_screen = Some(crate::tui::screens::LandingScreen::new());
    let now = Instant::now();
    report_startup_orphans(&mut app, repo, now, None);
    announce_quit_key(&mut app, now);
    let out = text(&mut app, 80, 24);
    (app, out)
}

#[test]
fn startup_orphan_notice_fits_the_first_frame_for_one_and_twelve_dirs() {
    let (_tmp, repo) = repo_with(&["leftover"]);
    let (_, out) = first_frame(&repo);
    let one = "untracked dir kept: .maestro/worktrees/leftover";
    assert!(out.contains(one), "N=1 first frame:\n{out}");

    let long = "unified-1563-1564-fix-core-startup-orphan-cleanup-never-re-1a2b";
    let names: Vec<String> = (1..=12).map(|i| format!("{long}-{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_tmp, repo) = repo_with(&names);
    let (_, out) = first_frame(&repo);
    let twelve = "12 untracked dirs kept · check: ls .maestro/worktrees";
    assert!(out.contains(twelve), "N=12 first frame:\n{out}");
    // The ⌃Q toast was dropped for it; the footer still pins ⌃Q quit.
    let footer = out.lines().last().unwrap_or_default();
    assert!(footer.contains("⌃Q quit"), "footer: {footer}");
}

#[tokio::test]
async fn esc_on_the_startup_notice_records_every_dir() {
    let names = ["session-1a2b3c4d", "issue-1563"];
    let (_tmp, repo) = repo_with(&names);
    let (mut app, _) = first_frame(&repo);
    let marked = |n: &str| worktree(&repo, n).join(SHOWN_MARKER).exists();
    assert!(!names.iter().any(|n| marked(n)), "not seen until dismissed");

    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let _ = handle_key(&mut app, esc).await;

    assert_eq!(app.tui_mode, TuiMode::Landing, "Esc only dismissed it");
    let out = text(&mut app, 80, 24);
    assert!(!out.contains("untracked dirs kept"), "dismissed:\n{out}");
    assert!(names.iter().all(|n| marked(n)), "Esc = seen for all");
    assert!(worktree(&repo, names[0]).join("file").exists(), "kept");
}
