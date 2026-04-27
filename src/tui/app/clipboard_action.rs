#![deny(clippy::unwrap_used)]

//! Copy-focused-response action: routes a `c` keypress on the Overview
//! into a clipboard write of the focused session's `last_message`, with
//! ANSI escapes stripped.
//!
//! All clipboard I/O goes through [`crate::tui::clipboard::Clipboard`] so
//! tests can swap in a `MockClipboard` (see `clipboard::testing`).

use crate::session::types::Session;
use crate::tui::app::App;
use crate::tui::clipboard::strip_ansi;
use crate::tui::clipboard_toast::CopyToastKind;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Toast TTL.
pub(crate) const COPY_TOAST_TTL_MS: u64 = 2_000;

#[derive(Debug, Clone)]
pub(crate) struct CopyToast {
    pub(crate) kind: CopyToastKind,
    pub(crate) message: String,
    set_at: Instant,
}

#[derive(Debug)]
pub(crate) enum CopyOutcome {
    Success,
    NoContent,
    NotEnded,
    Failed(String),
}

impl CopyOutcome {
    /// Map outcome → toast (kind + message). `None` for the silent paths;
    /// the dimmed hint already communicates the disabled state.
    pub(crate) fn toast(&self) -> Option<(CopyToastKind, String)> {
        match self {
            Self::Success => Some((
                CopyToastKind::Success,
                "Response copied to clipboard".to_string(),
            )),
            Self::Failed(msg) => Some((CopyToastKind::Error, format!("Copy failed: {msg}"))),
            Self::NoContent | Self::NotEnded => None,
        }
    }
}

impl App {
    /// True when the focused session has terminal status and non-empty
    /// `last_message`. Drives both the `c` key arm in the input handler
    /// and the dimmed-hint styling in the status bar.
    pub(crate) fn copy_focused_response_enabled(&self) -> bool {
        self.focused_session_for_copy()
            .map(|s| s.status.is_terminal() && !s.last_message.is_empty())
            .unwrap_or(false)
    }

    pub(crate) fn focused_session_for_copy(&self) -> Option<&Session> {
        self.pool.session_at_index(self.panel_view.selected_index())
    }

    /// Try to copy the focused session's `last_message` (ANSI-stripped) to
    /// the system clipboard via `self.clipboard`. Pure routing — toast
    /// state is set by the caller from `CopyOutcome::toast`.
    pub(crate) fn copy_focused_response(&self) -> CopyOutcome {
        let (id_for_log, payload) = {
            let Some(session) = self.focused_session_for_copy() else {
                return CopyOutcome::NoContent;
            };
            if !session.status.is_terminal() {
                return CopyOutcome::NotEnded;
            }
            if session.last_message.is_empty() {
                return CopyOutcome::NoContent;
            }
            (session.id, strip_ansi(&session.last_message))
        };
        match self.clipboard.write(&payload) {
            Ok(()) => {
                debug!(session_id = %id_for_log, "copied response to clipboard");
                CopyOutcome::Success
            }
            Err(e) => {
                // Verbose error stays in the trace log; the toast surfaces a
                // fixed message so it doesn't leak arboard internals (e.g.
                // Wayland socket paths) to screen-shares.
                warn!(error = %e, "clipboard write failed");
                CopyOutcome::Failed("clipboard unavailable".to_string())
            }
        }
    }

    pub(crate) fn set_copy_toast(&mut self, kind: CopyToastKind, message: String) {
        self.copy_toast = Some(CopyToast {
            kind,
            message,
            set_at: Instant::now(),
        });
    }

    /// Drop the toast if `now - set_at >= TTL`. The `now` arg is injected
    /// for deterministic testing; the production caller passes
    /// `Instant::now()`.
    pub(crate) fn tick_copy_toast(&mut self, now: Instant) {
        if let Some(t) = &self.copy_toast
            && now.duration_since(t.set_at) >= Duration::from_millis(COPY_TOAST_TTL_MS)
        {
            self.copy_toast = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{Session, SessionStatus};
    use crate::tui::app::App;
    use crate::tui::clipboard::testing::MockClipboard;
    use crate::tui::make_test_app as make_app;

    fn make_clipboard_app(mock: MockClipboard) -> App {
        make_app("maestro-clipboard-action-test").with_clipboard(Box::new(mock))
    }

    fn empty_app() -> App {
        make_clipboard_app(MockClipboard::new())
    }

    fn session_with(status: SessionStatus, last_message: &str) -> Session {
        let mut s = Session::new(
            "Test task".to_string(),
            "claude-opus-4-5".to_string(),
            "orchestrator".to_string(),
            None,
        );
        s.status = status;
        s.last_message = last_message.to_string();
        s
    }

    // ── CopyOutcome::toast ────────────────────────────────────────────

    #[test]
    fn copy_outcome_success_toast_is_some_with_confirmation_message() {
        match CopyOutcome::Success.toast() {
            Some((CopyToastKind::Success, msg)) => {
                assert_eq!(msg, "Response copied to clipboard")
            }
            other => panic!("unexpected toast: {other:?}"),
        }
    }

    #[test]
    fn copy_outcome_failed_toast_carries_error_kind_and_message() {
        match CopyOutcome::Failed("disk full".to_string()).toast() {
            Some((CopyToastKind::Error, msg)) => assert_eq!(msg, "Copy failed: disk full"),
            other => panic!("unexpected toast: {other:?}"),
        }
    }

    #[test]
    fn copy_outcome_no_content_toast_is_none() {
        assert!(CopyOutcome::NoContent.toast().is_none());
    }

    #[test]
    fn copy_outcome_not_ended_toast_is_none() {
        assert!(CopyOutcome::NotEnded.toast().is_none());
    }

    // ── copy_focused_response_enabled ─────────────────────────────────

    #[test]
    fn enabled_when_completed_and_has_content() {
        let mut app = empty_app();
        app.pool
            .enqueue(session_with(SessionStatus::Completed, "Some response"));
        app.panel_view.selected = Some(0);
        assert!(app.copy_focused_response_enabled());
    }

    #[test]
    fn disabled_when_completed_but_empty_last_message() {
        let mut app = empty_app();
        app.pool.enqueue(session_with(SessionStatus::Completed, ""));
        app.panel_view.selected = Some(0);
        assert!(!app.copy_focused_response_enabled());
    }

    #[test]
    fn disabled_when_running_regardless_of_content() {
        let mut app = empty_app();
        app.pool.enqueue(session_with(
            SessionStatus::Running,
            "Partial output so far",
        ));
        app.panel_view.selected = Some(0);
        assert!(!app.copy_focused_response_enabled());
    }

    #[test]
    fn enabled_when_needs_review_and_has_content() {
        let mut app = empty_app();
        app.pool
            .enqueue(session_with(SessionStatus::NeedsReview, "Review required"));
        app.panel_view.selected = Some(0);
        assert!(app.copy_focused_response_enabled());
    }

    #[test]
    fn enabled_when_killed_and_has_content() {
        let mut app = empty_app();
        app.pool
            .enqueue(session_with(SessionStatus::Killed, "Work done before kill"));
        app.panel_view.selected = Some(0);
        assert!(app.copy_focused_response_enabled());
    }

    #[test]
    fn disabled_when_no_focused_session() {
        let app = empty_app();
        assert!(!app.copy_focused_response_enabled());
    }

    #[test]
    fn disabled_when_errored_status() {
        let mut app = empty_app();
        app.pool
            .enqueue(session_with(SessionStatus::Errored, "Something went wrong"));
        app.panel_view.selected = Some(0);
        assert!(!app.copy_focused_response_enabled());
    }

    // ── copy_focused_response — outcomes ──────────────────────────────

    #[test]
    fn copy_success_writes_last_message_to_clipboard() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        app.pool
            .enqueue(session_with(SessionStatus::Completed, "hello"));
        app.panel_view.selected = Some(0);

        let outcome = app.copy_focused_response();

        assert!(matches!(outcome, CopyOutcome::Success));
        assert_eq!(mock.recorded_writes(), vec!["hello"]);
    }

    #[test]
    fn copy_no_content_does_not_write_to_clipboard() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        app.pool.enqueue(session_with(SessionStatus::Completed, ""));
        app.panel_view.selected = Some(0);

        let outcome = app.copy_focused_response();

        assert!(matches!(outcome, CopyOutcome::NoContent));
        assert_eq!(mock.write_count(), 0);
    }

    #[test]
    fn copy_not_ended_does_not_write_to_clipboard() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        app.pool
            .enqueue(session_with(SessionStatus::Running, "Streaming..."));
        app.panel_view.selected = Some(0);

        let outcome = app.copy_focused_response();

        assert!(matches!(outcome, CopyOutcome::NotEnded));
        assert_eq!(mock.write_count(), 0);
    }

    #[test]
    fn copy_failed_returns_error_outcome_and_does_not_panic() {
        // Failure must surface a fixed user-visible string and never leak
        // the inner error chain.
        let mock = MockClipboard::will_fail("internal: $XDG_RUNTIME_DIR/wayland-0");
        let mut app = make_clipboard_app(mock.clone());
        app.pool
            .enqueue(session_with(SessionStatus::Completed, "content"));
        app.panel_view.selected = Some(0);

        let outcome = app.copy_focused_response();

        match outcome {
            CopyOutcome::Failed(msg) => assert_eq!(msg, "clipboard unavailable"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(mock.write_count(), 0);
    }

    #[test]
    fn copy_strips_ansi_before_writing_to_clipboard() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        let ansi = "\x1b[32mSuccess\x1b[0m: all tests pass";
        app.pool
            .enqueue(session_with(SessionStatus::Completed, ansi));
        app.panel_view.selected = Some(0);

        let outcome = app.copy_focused_response();

        assert!(matches!(outcome, CopyOutcome::Success));
        let writes = mock.recorded_writes();
        assert_eq!(writes, vec!["Success: all tests pass"]);
        assert!(!writes[0].contains('\x1b'));
    }

    #[test]
    fn copy_writes_only_focused_tab_content() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        app.pool.enqueue(session_with(
            SessionStatus::Completed,
            "Content of session A",
        ));
        app.pool.enqueue(session_with(
            SessionStatus::Completed,
            "Content of session B",
        ));
        app.panel_view.selected = Some(1);

        let outcome = app.copy_focused_response();

        assert!(matches!(outcome, CopyOutcome::Success));
        assert_eq!(mock.recorded_writes(), vec!["Content of session B"]);
    }

    // ── Toast TTL ─────────────────────────────────────────────────────

    #[test]
    fn toast_cleared_when_ttl_expires() {
        let mut app = empty_app();
        app.set_copy_toast(CopyToastKind::Success, "Copied!".to_string());
        assert!(app.copy_toast.is_some());

        let past_ttl = Instant::now() + Duration::from_millis(COPY_TOAST_TTL_MS + 1);
        app.tick_copy_toast(past_ttl);

        assert!(app.copy_toast.is_none());
    }

    #[test]
    fn toast_preserved_within_ttl() {
        let mut app = empty_app();
        app.set_copy_toast(CopyToastKind::Success, "Copied!".to_string());

        let within = Instant::now() + Duration::from_millis(500);
        app.tick_copy_toast(within);

        assert!(app.copy_toast.is_some());
    }

    #[test]
    fn toast_cleared_at_ttl_boundary() {
        let mut app = empty_app();
        app.set_copy_toast(CopyToastKind::Success, "x".to_string());

        let at_ttl = Instant::now() + Duration::from_millis(COPY_TOAST_TTL_MS);
        app.tick_copy_toast(at_ttl);

        assert!(app.copy_toast.is_none());
    }
}
