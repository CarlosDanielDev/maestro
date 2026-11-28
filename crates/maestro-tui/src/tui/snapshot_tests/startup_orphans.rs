//! #1563: the untracked-dir line on the first screen of a default launch,
//! in the real launch order: `startup_cleanup` just before `tui::run`,
//! then the run loop's ⌃Q notice, then frames on Landing at 80x24.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::confirm_exit::text;
use super::log_drawer::log_app;
use crate::setup::orphan_tests::repo_with_leftover;
use crate::setup::report_startup_orphans;
use crate::tui::app::TuiMode;
use crate::tui::input_handler::{announce_quit_key, handle_key};

const LINE: &str = "untracked dir kept · rm -rf .maestro/worktrees/leftover";

#[tokio::test]
async fn startup_orphan_line_shows_on_landing_after_the_quit_notice() {
    let (_tmp, repo, leftover) = repo_with_leftover();
    let mut app = log_app(TuiMode::Landing);
    app.screen_state.quit_hints.ctrl_q_shown = false;
    // What `tui::run` sets for the `maestro` boot path.
    app.screen_state.landing_screen = Some(crate::tui::screens::LandingScreen::new());
    // The launch happened 4s ago: past the 3s ⌃Q info toast, so `ui::draw`
    // (wall clock) draws the frame the user sees once it has gone.
    let launch = Instant::now()
        .checked_sub(Duration::from_secs(4))
        .expect("clock past 4s");

    report_startup_orphans(&mut app, &repo, launch, None);
    announce_quit_key(&mut app, launch);

    let first = (app.toasts.visible(launch)).map(|v| v.message.to_string());
    assert!(
        first.is_some_and(|m| m.starts_with("⌃Q quits now")),
        "the ⌃Q notice comes first"
    );
    let out = text(&mut app, 80, 24);
    assert!(out.contains(LINE), "path + remove command on Landing:\n{out}");
    assert!(leftover.exists());

    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let _ = handle_key(&mut app, esc).await;
    assert_eq!(app.tui_mode, TuiMode::Landing, "Esc only dismissed it");
    let out = text(&mut app, 80, 24);
    assert!(!out.contains(LINE), "Esc dismisses the line:\n{out}");
}
