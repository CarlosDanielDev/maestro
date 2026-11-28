//! App side of `ds::toast` (#1254): the toast queue on `App`, the global
//! `Ctrl+Z` undo and the `Esc` that dismisses a sticky error toast.

use std::time::Instant;

use crate::tui::app::App;
use crate::tui::ds::toast::Toast;

/// What `Ctrl+Z` runs while an undo toast is visible. Each screen issue
/// decides which actions get one.
pub(crate) type ToastUndo = Box<dyn FnOnce(&mut App)>;

impl App {
    /// Show `toast`. The first non-test caller lands with a screen issue.
    #[allow(dead_code)]
    pub(crate) fn push_toast(&mut self, toast: Toast<ToastUndo>, now: Instant) {
        self.toasts.push(toast, now);
    }

    /// Show `toast` on the first frame (#1563, the startup notice). The
    /// ⌃Q launch hint is logged now (the run loop's call is then a no-op)
    /// and its info toast dropped, because a live info toast hides every
    /// error toast. The footer still pins `⌃Q quit`.
    pub(crate) fn push_first_frame_toast(&mut self, toast: Toast<ToastUndo>, now: Instant) {
        crate::tui::input_handler::announce_quit_key(self, now);
        self.toasts.drop_transient();
        self.push_toast(toast, now);
    }

    /// `Ctrl+Z`: run the visible toast's undo. `false` (key not used) when
    /// no undo toast is visible or its window has closed.
    pub(crate) fn toast_ctrl_z(&mut self, now: Instant) -> bool {
        // The copy toast is drawn on top; never act on a hidden toast.
        if self.copy_toast.is_some() {
            return false;
        }
        let Some(undo) = self.toasts.take_undo(now) else {
            return false;
        };
        undo(self);
        true
    }

    /// `Esc`: dismiss the visible sticky error toast. `false` when none.
    pub(crate) fn toast_esc(&mut self, now: Instant) -> bool {
        let shown = (self.toasts.visible(now)).map(|v| v.message.to_string());
        let dismissed = self.copy_toast.is_none() && self.toasts.dismiss_error(now);
        // #1292: a dismissed conflict toast's keys go with it.
        if let (true, Some(message)) = (dismissed, shown) {
            self.forget_conflict_prompt(&message);
            self.ack_startup_orphans(&message);
        }
        dismissed
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::tui::ds::toast::Severity;
    use crate::tui::input_handler::handle_key;

    fn ctrl_z() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL)
    }

    fn undo_toast(ran: &Rc<Cell<bool>>) -> Toast<ToastUndo> {
        let ran = Rc::clone(ran);
        Toast::undoable("Paused #407", Box::new(move |_: &mut App| ran.set(true)))
    }

    #[tokio::test]
    async fn toast_ctrl_z_runs_undo_inside_window() {
        let mut app = crate::tui::make_test_app("maestro-toast-undo-inside");
        let ran = Rc::new(Cell::new(false));
        app.push_toast(undo_toast(&ran), Instant::now());

        let _ = handle_key(&mut app, ctrl_z()).await;

        assert!(ran.get(), "Ctrl+Z inside the window must run the undo");
        assert!(app.toasts.visible(Instant::now()).is_none());
    }

    #[tokio::test]
    async fn toast_ctrl_z_after_window_is_noop() {
        let mut app = crate::tui::make_test_app("maestro-toast-undo-after");
        let ran = Rc::new(Cell::new(false));
        let past = Instant::now()
            .checked_sub(Duration::from_secs(10))
            .expect("monotonic clock is past 10 s");
        app.push_toast(undo_toast(&ran), past);

        let _ = handle_key(&mut app, ctrl_z()).await;

        assert!(!ran.get(), "Ctrl+Z after the window must do nothing");
    }

    #[test]
    fn toast_ctrl_z_without_undo_toast_is_not_used() {
        let mut app = crate::tui::make_test_app("maestro-toast-undo-none");
        let now = Instant::now();
        assert!(!app.toast_ctrl_z(now));
        app.push_toast(Toast::new(Severity::Info, "plain"), now);
        assert!(!app.toast_ctrl_z(now), "a plain toast has nothing to undo");
    }

    #[test]
    fn toast_keys_ignore_queue_hidden_by_copy_toast() {
        use crate::tui::clipboard_toast::CopyToastKind;
        let mut app = crate::tui::make_test_app("maestro-toast-hidden");
        let now = Instant::now();
        let ran = Rc::new(Cell::new(false));
        app.push_toast(undo_toast(&ran), now);
        app.set_copy_toast(CopyToastKind::Success, "Copied".into());
        assert!(!app.toast_ctrl_z(now), "undo toast is hidden");
        assert!(!ran.get());
        app.push_toast(Toast::new(Severity::Error, "boom"), now);
        assert!(
            !app.toast_esc(now + Duration::from_secs(9)),
            "error is hidden"
        );
    }

    #[tokio::test]
    async fn esc_dismisses_visible_error_toast() {
        let mut app = crate::tui::make_test_app("maestro-toast-esc");
        app.push_toast(Toast::new(Severity::Error, "boom"), Instant::now());

        let _ = handle_key(&mut app, KeyEvent::from(KeyCode::Esc)).await;

        assert!(app.toasts.visible(Instant::now()).is_none());
    }

    #[tokio::test]
    async fn esc_reaches_the_screen_when_no_toast_is_visible() {
        use crate::tui::app::TuiMode;
        let mut app = crate::tui::make_test_app("maestro-toast-esc-passthrough");
        app.navigate_to(TuiMode::SessionSummary);
        assert_eq!(app.tui_mode, TuiMode::SessionSummary);

        let _ = handle_key(&mut app, KeyEvent::from(KeyCode::Esc)).await;

        assert_ne!(
            app.tui_mode,
            TuiMode::SessionSummary,
            "with no toast, Esc must go back as before"
        );
    }

    #[tokio::test]
    async fn esc_on_error_toast_only_dismisses_it() {
        use crate::tui::app::TuiMode;
        let mut app = crate::tui::make_test_app("maestro-toast-esc-first");
        app.navigate_to(TuiMode::SessionSummary);
        app.push_toast(Toast::new(Severity::Error, "boom"), Instant::now());

        let _ = handle_key(&mut app, KeyEvent::from(KeyCode::Esc)).await;

        assert_eq!(app.tui_mode, TuiMode::SessionSummary);
        assert!(app.toasts.visible(Instant::now()).is_none());
    }
}
