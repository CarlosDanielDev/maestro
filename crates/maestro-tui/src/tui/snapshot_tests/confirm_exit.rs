//! #1260: every exit chord goes through one Confirm Exit. With live agents
//! it needs a hold of ⌃Q (or a second ⌃Q inside the window); with an idle
//! fleet ⌃Q and `q` quit at once. The popup is sized from its content, so
//! the question and its key row show at 80 columns (B1, B2 of #1394).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::log_drawer::{draw, log_app};
use super::make_session;
use crate::session::types::SessionStatus;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::key_hints::QUIT;
use crate::tui::input_handler::{KeyAction, announce_quit_key, handle_key};

const RUNNING: &str = "session(s) still running. Exit anyway?";
const IDLE: &str = "Are you sure you want to exit?";

pub(super) fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

pub(super) fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// `mode` with `live` running sessions; the one-time ⌃Q hint is already seen.
pub(super) fn app(mode: TuiMode, live: usize) -> App {
    let mut app = log_app(mode);
    for i in 0..live {
        app.pool
            .enqueue(make_session(SessionStatus::Running, Some(400 + i as u64)));
    }
    app.screen_state.quit_hints.ctrl_q_shown = true;
    app.screen_state.quit_clock = Some(fixed_clock);
    app
}

/// No wall clock in these tests: every key and draw reads one instant.
pub(super) fn fixed_clock() -> Instant {
    static T0: OnceLock<Instant> = OnceLock::new();
    *T0.get_or_init(Instant::now)
}

/// [`fixed_clock`] plus `LATE_MS`, for the window-expiry test only.
static LATE_MS: AtomicU64 = AtomicU64::new(0);
fn late_clock() -> Instant {
    fixed_clock() + Duration::from_millis(LATE_MS.load(Ordering::SeqCst))
}

/// `mode` (Home or Dashboard, reached as a user does) with `open` pressed.
async fn opened(mode: TuiMode, live: usize, open: KeyEvent) -> App {
    let mut app = app(TuiMode::Landing, live);
    if mode == TuiMode::Dashboard {
        let _ = handle_key(&mut app, key('2')).await;
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
    }
    let _ = handle_key(&mut app, open).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit, "{mode:?} {open:?}");
    app
}

pub(super) fn text(app: &mut App, w: u16, h: u16) -> String {
    format!("{}", draw(app, w, h).backend())
}

/// The rows of the Confirm Exit box, top border to bottom border.
fn popup_rows(out: &str) -> Vec<String> {
    let rows: Vec<&str> = out.lines().collect();
    let top = rows
        .iter()
        .position(|r| r.contains("[ Confirm Exit ]"))
        .unwrap_or_else(|| panic!("no Confirm Exit box:\n{out}"));
    // The last corner left of the title: a panel border under the box
    // can share its top row (#1280 Dashboard detail pane).
    let title_at = rows[top].find("[ Confirm Exit ]").expect("box title");
    let col = rows[top][..title_at]
        .chars()
        .collect::<Vec<_>>()
        .iter()
        .rposition(|c| matches!(c, '┌' | '╔' | '╭'))
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

/// The TurboQuant notice shows once per launch (the run loop calls
/// `announce_quit_key` before the first frame) and never blocks a quit:
/// the first ⌃Q of an idle fleet quits.
#[tokio::test]
async fn ctrl_q_one_time_hint() {
    let mut app = app(TuiMode::Landing, 0);
    app.screen_state.quit_hints.ctrl_q_shown = false;
    let tq_before = format!("{:?}", app.flags);

    announce_quit_key(&mut app, fixed_clock());
    let out = text(&mut app, 80, 24);
    assert!(
        out.contains("Settings › TurboQuant"),
        "notice drawn:\n{out}"
    );
    assert!(app.screen_state.quit_hints.ctrl_q_shown);
    let toasts = app.activity_log.entries().len();
    announce_quit_key(&mut app, fixed_clock());
    assert_eq!(app.activity_log.entries().len(), toasts, "once per launch");

    let action = handle_key(&mut app, ctrl('q')).await;
    assert!(matches!(action, KeyAction::Quit), "the first ⌃Q quits");
    assert!(!app.running);
    assert_eq!(
        format!("{:?}", app.flags),
        tq_before,
        "⌃Q no longer toggles TQ"
    );
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

/// B1: the box holds its question and key row at 80x24 and 80x27, from
/// Home and Dashboard. An idle `q` quits at once, so the idle dialog comes
/// from ⌃C; with one running session it comes from `q`.
#[tokio::test]
async fn confirm_exit_popup_fits_content_at_80x24_dashboard() {
    for mode in [TuiMode::Landing, TuiMode::Dashboard] {
        for (live, open, want, keys) in [
            (0, ctrl('c'), IDLE, "[y]es"),
            (1, key('q'), RUNNING, "hold to quit"),
        ] {
            for h in [24, 27] {
                let mut app = opened(mode, live, open).await;
                let out = text(&mut app, 80, h);
                let rows = popup_rows(&out).join("\n");
                assert!(rows.contains(want), "{mode:?} 80x{h}, {live} live:\n{out}");
                assert!(rows.contains(keys), "{mode:?} 80x{h}, {live} live:\n{out}");
            }
        }
    }
}

/// B2: every key row is drawn between the top and bottom borders, at
/// 80x24 and 80x27, from Home and Dashboard.
#[tokio::test]
async fn confirm_exit_popup_shows_key_hint_row() {
    let cases: [(usize, KeyEvent, &str, &[&str]); 2] = [
        (0, ctrl('c'), IDLE, &["[y]es", "[n]o", "[Esc]"]),
        (
            1,
            key('q'),
            "1 session(s) still running",
            &["hold to quit", "Esc"],
        ),
    ];
    for mode in [TuiMode::Landing, TuiMode::Dashboard] {
        for (live, open, question, keys) in cases {
            for h in [24, 27] {
                let mut app = opened(mode, live, open).await;
                let out = text(&mut app, 80, h);
                let rows = popup_rows(&out);
                let inner = &rows[1..rows.len() - 1];
                let at = |s: &str| inner.iter().any(|r| r.contains(s));
                assert!(at(question), "{mode:?} 80x{h}:\n{out}");
                for k in keys {
                    assert!(at(k), "{mode:?} 80x{h}: {k} inside the box:\n{out}");
                }
            }
        }
    }
}

/// No kitty protocol (plain terminal, tmux): no release events, and key
/// auto-repeat arrives as more `Press` events. A late second ⌃Q only
/// re-arms; one inside the window quits. The clock is injected.
#[tokio::test]
async fn exit_hold_press_only_window_on_injected_clock() {
    let mut app = app(TuiMode::Dashboard, 1);
    app.screen_state.quit_clock = Some(late_clock);
    LATE_MS.store(0, Ordering::SeqCst);
    let press = ctrl('q'); // KeyEvent::new = KeyEventKind::Press
    let _ = handle_key(&mut app, press).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    LATE_MS.store(2_500, Ordering::SeqCst);
    let action = handle_key(&mut app, press).await;
    assert!(matches!(action, KeyAction::Consumed), "late press re-arms");
    assert!(app.running);
    LATE_MS.store(2_530, Ordering::SeqCst); // an auto-repeat 30 ms later
    let action = handle_key(&mut app, press).await;
    assert!(matches!(action, KeyAction::Quit), "press inside the window");
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

/// #1423: a session a quit cannot save for `maestro resume` (an adopted
/// conversation: it runs outside any worktree) is named in Confirm Exit,
/// before the quit, at 80x24. Sessions resume can bring back are not.
#[tokio::test]
async fn confirm_exit_names_sessions_quit_would_kill() {
    const KILLS: &str = "Quit ends (no resume):";
    let mut app = app(TuiMode::Landing, 1);
    let _ = handle_key(&mut app, ctrl('x')).await;
    let out = text(&mut app, 80, 24);
    assert!(!out.contains(KILLS), "nothing to kill:\n{out}");

    let mut app = self::app(TuiMode::Landing, 1);
    let cwd = tempfile::tempdir().unwrap();
    let adopted = (app.pool)
        .create_adopted_session(
            "eeeeeeee-0000-0000-0000-000000001423",
            cwd.path().to_path_buf(),
            None,
            "opus".into(),
        )
        .expect("adopted")
        .session_id;
    let _ = handle_key(&mut app, ctrl('x')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    let out = text(&mut app, 80, 24);
    let row = (popup_rows(&out).into_iter())
        .find(|r| r.contains(KILLS))
        .unwrap_or_else(|| panic!("the adopted session is not named:\n{out}"));
    assert!(
        row.contains(&format!("S-{}", &adopted.to_string()[..8])),
        "{row}"
    );
    assert!(
        !row.contains("#400"),
        "a resumable session is not named: {row}"
    );
}
