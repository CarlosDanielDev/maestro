//! #1563: the untracked-dir notice in the real launch order of `maestro`:
//! `startup_cleanup` just before `tui::run`, then the run loop's
//! `announce_quit_key`, then the FIRST frame on Landing at 80x24.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::confirm_exit::text;
use super::log_drawer::log_app;
use crate::setup::orphans::{SHOWN_MARKER, remove_command};
use crate::setup::orphans::tests::{repo_with, worktree};
use crate::setup::report_startup_orphans;
use crate::tui::app::TuiMode;
use crate::tui::input_handler::{announce_quit_key, handle_key};

/// A `session_slug` name, and a 63-char one (a branch-sized name).
const NAMES: [&str; 2] = [
    "session-1a2b3c4d",
    "unified-1563-1564-fix-core-startup-orphan-cleanup-never-re-1a2b",
];
const NOTICE: &str = "kept 2 untracked dirs · ⌃L shows how to remove";

#[tokio::test]
async fn startup_orphan_notice_on_the_first_frame_and_full_commands_on_ctrl_l() {
    let (_tmp, repo) = repo_with(&NAMES);
    let mut app = log_app(TuiMode::Landing);
    app.screen_state.quit_hints.ctrl_q_shown = false;
    // What `tui::run` sets for the `maestro` boot path.
    app.screen_state.landing_screen = Some(crate::tui::screens::LandingScreen::new());
    let now = Instant::now();

    report_startup_orphans(&mut app, &repo, now, None);
    announce_quit_key(&mut app, now);

    let first = text(&mut app, 80, 24);
    assert!(first.contains(NOTICE), "first frame:\n{first}");
    // The ⌃Q toast was dropped for it; the footer still pins ⌃Q quit.
    let footer = first.lines().last().unwrap_or_default();
    assert!(footer.contains("⌃Q quit"), "footer: {footer}");

    let ctrl_l = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_l).await;
    let drawer = text(&mut app, 80, 24);
    assert!(!drawer.contains('…'), "nothing cut:\n{drawer}");
    for name in NAMES {
        let pieces = remove_command(name);
        assert_eq!(pieces.concat(), format!("rm -rf .maestro/worktrees/{name}"));
        for (i, piece) in pieces.iter().enumerate() {
            let row = if i + 1 < pieces.len() { format!("{piece}\\") } else { piece.clone() };
            assert!(drawer.contains(&row), "{row:?} in the drawer:\n{drawer}");
        }
    }
    let marker = worktree(&repo, NAMES[0]).join(SHOWN_MARKER);
    assert!(!marker.exists(), "seen in the log is not dismissed yet");

    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let _ = handle_key(&mut app, esc).await; // closes the drawer
    let _ = handle_key(&mut app, esc).await; // dismisses the notice
    assert_eq!(app.tui_mode, TuiMode::Landing, "Esc only closed overlays");
    let out = text(&mut app, 80, 24);
    assert!(!out.contains(NOTICE), "dismissed:\n{out}");
    assert!(marker.exists(), "dismissed = shown, recorded");
    assert!(worktree(&repo, NAMES[1]).join("file").exists(), "kept");
}
