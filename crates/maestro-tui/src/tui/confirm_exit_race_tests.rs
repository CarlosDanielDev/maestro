//! #1158: a session that finishes while "Exit anyway?" is open must not
//! steal the dialog's focus, so the user's `y` still quits.

use super::input_handler::{KeyAction, handle_key};
use super::{app::TuiMode, completion_ready, make_test_app};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// App on Overview with one running session.
fn app_with_running_session() -> (super::app::App, uuid::Uuid) {
    let mut app = make_test_app("confirm-exit-race-1158");
    app.tui_mode = TuiMode::Overview;
    let session = crate::session::types::Session::new(
        "prompt".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    let id = session.id;
    app.pool.enqueue(session);
    let _ = app.pool.try_promote();
    (app, id)
}

/// The event-loop tail that opens the summary once every session is done.
fn deliver_completion(app: &mut super::app::App, id: uuid::Uuid) {
    app.pool.finalize_retain_worktree(id);
    if completion_ready(app, true) {
        app.open_completion_summary();
    }
}

#[tokio::test]
async fn y_after_completion_under_confirm_exit_quits() {
    let (mut app, id) = app_with_running_session();
    handle_key(&mut app, key('q')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);

    deliver_completion(&mut app, id);
    let action = handle_key(&mut app, key('y')).await;

    assert!(matches!(action, KeyAction::Quit), "`y` must quit");
    assert!(!app.running);
}

#[tokio::test]
async fn completion_under_confirm_exit_keeps_dialog_focused() {
    let (mut app, id) = app_with_running_session();
    handle_key(&mut app, key('q')).await;

    deliver_completion(&mut app, id);

    assert_eq!(
        app.tui_mode,
        TuiMode::ConfirmExit,
        "completion must not change which screen gets the next key"
    );
    assert!(app.completion_summary.is_some(), "summary is still built");

    // Cancelling the dialog lands on the summary, then back to Overview.
    handle_key(&mut app, key('n')).await;
    assert_eq!(app.tui_mode, TuiMode::CompletionSummary);
    assert!(app.running);
    assert_eq!(app.nav_stack.peek(), Some(&TuiMode::Overview));
}

/// #1260: `ds::confirm::HoldToConfirm` races. Terminals send no key release
/// without the kitty protocol, so a hold is a run of repeats and a second
/// press inside the window is the fallback. `⏎` never confirms.
mod hold {
    use std::time::{Duration, Instant};

    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use crate::tui::ds::confirm::{ConfirmOutcome, HoldToConfirm};

    fn ctrl_q(kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::CONTROL,
            kind,
            state: KeyEventState::NONE,
        }
    }

    fn hold() -> HoldToConfirm {
        HoldToConfirm::new(KeyCode::Char('q'), KeyModifiers::CONTROL)
    }

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn hold_first_press_only_arms() {
        let t0 = Instant::now();
        let mut h = hold();
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Press), t0),
            ConfirmOutcome::Pending
        );
        assert!(h.is_armed(t0));
        assert!(h.progress(t0) > 0.0 && h.progress(t0) < 1.0);
    }

    #[test]
    fn hold_second_press_inside_window_confirms() {
        let t0 = Instant::now();
        let mut h = hold();
        h.handle_key(&ctrl_q(KeyEventKind::Press), t0);
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Press), at(t0, 1_900)),
            ConfirmOutcome::Confirmed
        );
    }

    #[test]
    fn hold_second_press_after_window_only_rearms() {
        let t0 = Instant::now();
        let mut h = hold();
        h.handle_key(&ctrl_q(KeyEventKind::Press), t0);
        let late = at(t0, 2_100);
        assert!(!h.is_armed(late), "the window closed");
        assert_eq!(h.progress(late), 0.0);
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Press), late),
            ConfirmOutcome::Pending
        );
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Press), at(t0, 2_200)),
            ConfirmOutcome::Confirmed
        );
    }

    #[test]
    fn hold_repeats_confirm_only_after_hold_duration() {
        let t0 = Instant::now();
        let mut h = hold();
        h.handle_key(&ctrl_q(KeyEventKind::Press), t0);
        for ms in [100, 250, 400, 490] {
            assert_eq!(
                h.handle_key(&ctrl_q(KeyEventKind::Repeat), at(t0, ms)),
                ConfirmOutcome::Pending,
                "repeat at {ms} ms"
            );
        }
        let mid = h.progress(at(t0, 490));
        assert!(mid > 0.5 && mid < 1.0, "the bar fills while held: {mid}");
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Repeat), at(t0, 500)),
            ConfirmOutcome::Confirmed
        );
    }

    #[test]
    fn hold_release_never_counts_as_a_press() {
        // Windows reports a release for every tap: one tap must not confirm.
        let t0 = Instant::now();
        let mut h = hold();
        h.handle_key(&ctrl_q(KeyEventKind::Press), t0);
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Release), at(t0, 50)),
            ConfirmOutcome::Pending
        );
        let mut cold = hold();
        assert_eq!(
            cold.handle_key(&ctrl_q(KeyEventKind::Release), t0),
            ConfirmOutcome::Pending
        );
        assert!(!cold.is_armed(t0), "a release does not arm");
    }

    #[test]
    fn hold_enter_alone_is_inert() {
        let t0 = Instant::now();
        let mut h = hold();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        for ms in [0, 100, 200] {
            assert_eq!(h.handle_key(&enter, at(t0, ms)), ConfirmOutcome::Pending);
        }
        assert!(!h.is_armed(at(t0, 200)), "Enter does not arm");
        h.handle_key(&ctrl_q(KeyEventKind::Press), at(t0, 300));
        assert_eq!(
            h.handle_key(&enter, at(t0, 400)),
            ConfirmOutcome::Pending,
            "Enter does not finish an armed hold"
        );
    }

    #[test]
    fn hold_other_keys_do_not_count() {
        let t0 = Instant::now();
        let mut h = hold();
        h.handle_key(&ctrl_q(KeyEventKind::Press), t0);
        for key in [
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(
                h.handle_key(&key, at(t0, 100)),
                ConfirmOutcome::Pending,
                "{key:?}"
            );
        }
    }

    #[test]
    fn hold_esc_cancels_and_reset_disarms() {
        let t0 = Instant::now();
        let mut h = hold();
        h.handle_key(&ctrl_q(KeyEventKind::Press), t0);
        assert_eq!(
            h.handle_key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), t0),
            ConfirmOutcome::Cancelled
        );
        h.reset();
        assert!(!h.is_armed(t0));
        assert_eq!(
            h.handle_key(&ctrl_q(KeyEventKind::Press), at(t0, 100)),
            ConfirmOutcome::Pending,
            "after a reset the next press only arms"
        );
    }
}
