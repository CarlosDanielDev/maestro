//! #1260: every exit chord goes through one Confirm Exit. With live agents
//! it needs a hold of ⌃Q (or a second ⌃Q inside the window); with an idle
//! fleet ⌃Q and `q` quit at once. The popup is sized from its content, so
//! the question and its key row show at 80 columns (B1, B2 of #1394).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::log_drawer::{draw, log_app};
use super::make_session;
use crate::session::types::SessionStatus;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::key_hints::QUIT;
use crate::tui::input_handler::{KeyAction, handle_key};

const RUNNING: &str = "session(s) still running. Exit anyway?";
const IDLE: &str = "Are you sure you want to exit?";

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// `mode` with `live` running sessions; the one-time ⌃Q hint is already seen.
fn app(mode: TuiMode, live: usize) -> App {
    let mut app = log_app(mode);
    for i in 0..live {
        app.pool
            .enqueue(make_session(SessionStatus::Running, Some(400 + i as u64)));
    }
    app.screen_state.quit_hints.ctrl_q_shown = true;
    app
}

fn text(app: &mut App, w: u16, h: u16) -> String {
    format!("{}", draw(app, w, h).backend())
}

/// The rows of the Confirm Exit box, top border to bottom border.
fn popup_rows(out: &str) -> Vec<String> {
    let rows: Vec<&str> = out.lines().collect();
    let top = rows
        .iter()
        .position(|r| r.contains("[ Confirm Exit ]"))
        .unwrap_or_else(|| panic!("no Confirm Exit box:\n{out}"));
    let col = rows[top]
        .chars()
        .position(|c| matches!(c, '┌' | '╔' | '╭'))
        .expect("top-left corner");
    let bottom = (top + 1..rows.len())
        .find(|&i| matches!(rows[i].chars().nth(col), Some('└' | '╚' | '╰')))
        .unwrap_or_else(|| panic!("no bottom border:\n{out}"));
    rows[top..=bottom].iter().map(|r| r.to_string()).collect()
}

#[tokio::test]
async fn exit_chords_route_to_confirm_exit() {
    for chord in [ctrl('c'), ctrl('x'), ctrl('q'), key('q')] {
        for mode in [TuiMode::Landing, TuiMode::Dashboard] {
            let mut app = app(mode, 2);
            let action = handle_key(&mut app, chord).await;
            assert!(matches!(action, KeyAction::Consumed), "{chord:?} quit");
            assert!(app.running, "{chord:?} on {mode:?} exited directly");
            assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "{chord:?} {mode:?}");
            let out = text(&mut app, 80, 24);
            assert!(out.contains(&format!("2 {RUNNING}")), "{chord:?}:\n{out}");

            // `y` and a reflex Enter no longer quit while agents run.
            for k in [key('y'), KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)] {
                let action = handle_key(&mut app, k).await;
                assert!(matches!(action, KeyAction::Consumed), "{k:?} quit");
                assert!(app.running);
            }
        }
    }
}

#[tokio::test]
async fn exit_hold_double_tap_quits_and_esc_stays() {
    let mut app = app(TuiMode::Dashboard, 1);
    let _ = handle_key(&mut app, ctrl('q')).await;
    let action = handle_key(&mut app, ctrl('q')).await;
    assert!(matches!(action, KeyAction::Quit), "second ⌃Q in the window");
    assert!(!app.running);

    let mut app = self::app(TuiMode::Dashboard, 1);
    let _ = handle_key(&mut app, ctrl('x')).await;
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard, "Esc stays");
    assert!(app.running);
    // The next ⌃Q opens a fresh dialog: it arms, it does not quit.
    let action = handle_key(&mut app, ctrl('q')).await;
    assert!(matches!(action, KeyAction::Consumed));
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
}

#[tokio::test]
async fn idle_fleet_quits_directly() {
    for chord in [ctrl('q'), key('q')] {
        for mode in [TuiMode::Landing, TuiMode::Dashboard] {
            let mut app = app(mode, 0);
            let action = handle_key(&mut app, chord).await;
            assert!(matches!(action, KeyAction::Quit), "{chord:?} {mode:?}");
            assert!(!app.running, "{chord:?} {mode:?}");
        }
    }
    // ⌃C and ⌃X still ask, then `y` quits an idle fleet.
    for chord in [ctrl('c'), ctrl('x')] {
        let mut app = app(TuiMode::Landing, 0);
        let _ = handle_key(&mut app, chord).await;
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "{chord:?}");
        let action = handle_key(&mut app, key('y')).await;
        assert!(matches!(action, KeyAction::Quit), "{chord:?} then y");
    }
}

#[tokio::test]
async fn ctrl_q_one_time_hint() {
    let mut app = app(TuiMode::Landing, 0);
    app.screen_state.quit_hints.ctrl_q_shown = false;
    let tq_before = format!("{:?}", app.flags);

    let action = handle_key(&mut app, ctrl('q')).await;
    assert!(
        matches!(action, KeyAction::Consumed),
        "first ⌃Q must not quit"
    );
    assert!(app.running);
    assert_eq!(app.tui_mode, TuiMode::Landing);
    assert_eq!(
        format!("{:?}", app.flags),
        tq_before,
        "⌃Q no longer toggles TQ"
    );
    let out = text(&mut app, 80, 24);
    assert!(out.contains("Settings › TurboQuant"), "hint drawn:\n{out}");

    let action = handle_key(&mut app, ctrl('q')).await;
    assert!(matches!(action, KeyAction::Quit), "the hint shows once");
}

#[tokio::test]
async fn first_ctrl_x_points_to_ctrl_q() {
    let mut app = app(TuiMode::Landing, 1);
    let _ = handle_key(&mut app, ctrl('x')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    let out = text(&mut app, 80, 24);
    assert!(out.contains("⌃Q quits"), "one-time ⌃X hint:\n{out}");
    assert!(app.screen_state.quit_hints.ctrl_x_shown);
}

#[tokio::test]
async fn footer_pins_quit() {
    assert_eq!(QUIT, ("⌃Q", "quit"));
    for mode in [TuiMode::Landing, TuiMode::Dashboard] {
        let mut app = app(mode, 0);
        let out = text(&mut app, 80, 24);
        let footer = out.lines().last().expect("footer row");
        let footer = footer.trim_matches('"').trim_end();
        assert!(footer.ends_with("⌃Q quit"), "{mode:?}: {footer:?}");
    }
}

#[tokio::test]
async fn confirm_exit_popup_fits_content_at_80x24_dashboard() {
    // An idle `q` quits at once, so the idle dialog comes from ⌃C.
    for (live, open, want) in [(0, ctrl('c'), IDLE), (1, key('q'), RUNNING)] {
        for h in [24, 27] {
            let mut app = app(TuiMode::Landing, live);
            let _ = handle_key(&mut app, key('2')).await;
            assert_eq!(app.tui_mode, TuiMode::Dashboard);
            let _ = handle_key(&mut app, open).await;
            assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
            let out = text(&mut app, 80, h);
            let rows = popup_rows(&out).join("\n");
            assert!(rows.contains(want), "80x{h}, {live} live:\n{out}");
            let keys = if live == 0 { "[y]es" } else { "hold to quit" };
            assert!(rows.contains(keys), "80x{h}, {live} live:\n{out}");
        }
    }
}

#[tokio::test]
async fn confirm_exit_popup_shows_key_hint_row() {
    // With an agent running, `q` on Home opens the hold dialog; an idle
    // `q` quits at once, so the idle dialog comes from ⌃C.
    let cases: [(usize, KeyEvent, &str, &str); 2] = [
        (1, key('q'), RUNNING, "hold to quit"),
        (0, ctrl('c'), IDLE, "[y]es"),
    ];
    for (live, open, question, keys) in cases {
        for h in [24, 27] {
            let mut app = app(TuiMode::Landing, live);
            let _ = handle_key(&mut app, open).await;
            assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
            let out = text(&mut app, 80, h);
            let rows = popup_rows(&out);
            let inner = &rows[1..rows.len() - 1];
            assert!(inner.iter().any(|r| r.contains(question)), "80x{h}:\n{out}");
            assert!(
                inner.iter().any(|r| r.contains(keys)),
                "80x{h}: key row inside the box:\n{out}"
            );
            assert!(
                inner.iter().any(|r| r.contains("Esc")),
                "80x{h}: Esc row inside the box:\n{out}"
            );
        }
    }
}

#[tokio::test]
async fn confirm_exit_hold() {
    let mut app = app(TuiMode::Dashboard, 2);
    app.show_activity_log = false;
    let _ = handle_key(&mut app, ctrl('q')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    let terminal = draw(&mut app, 80, 24);
    let out = format!("{}", terminal.backend());
    assert!(out.contains(&format!("2 {RUNNING}")), "agent count:\n{out}");
    assert!(
        out.contains("hold to quit") && out.contains('█'),
        "progress:\n{out}"
    );
    insta::assert_snapshot!("confirm_exit_hold", terminal.backend());
}
