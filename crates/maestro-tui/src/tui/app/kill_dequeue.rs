//! #1556: Kill on a queued session removes it the way its dialog said.

use crate::tui::app::App;
use crate::tui::app::helpers::session_label;
use crate::session::pool::Dequeued;
use crate::tui::ds::status_bar::Place;
use crate::tui::ds::toast::{Severity, Toast};

impl App {
    /// #1556: take a queued `session_id` out of the queue the way its open
    /// dialog said ([`SessionPool::dequeue_as_shown`]); a dialog this app
    /// did not open reads the pool's answer now. A worktree kept against
    /// the pool's answer, or one git failed to remove, gets a toast: the
    /// log alone is not enough. `None` when the session is not queued;
    /// `Some(false)` when it is queued but cannot become `Killed`.
    ///
    /// [`SessionPool::dequeue_as_shown`]: crate::session::pool::SessionPool::dequeue_as_shown
    pub(crate) fn dequeue_as_shown(&mut self, session_id: uuid::Uuid) -> Option<bool> {
        let shown = match self.screen_state.kill_teardown_shown.take() {
            Some((id, shown)) if id == session_id => shown,
            _ => self.pool.dequeue_tears_down(session_id),
        };
        if self.place_of(session_id) != Some(Place::Queued) {
            return None;
        }
        let session = self.pool.get_session(session_id);
        let label = session.map_or_else(|| "The session".to_string(), session_label);
        let path = session.and_then(|s| s.worktree_path.clone());
        let Some(outcome) = self.pool.dequeue_as_shown(session_id, shown) else {
            return Some(false);
        };
        let path = path.map_or_else(|| "its worktree".to_string(), |p| p.display().to_string());
        let toast = match outcome {
            Dequeued::Untouched | Dequeued::Removed => return Some(true),
            Dequeued::KeptAsShown => Toast::new(
                Severity::Warn,
                format!("{label} left the queue; its worktree was kept, as the dialog said: {path}"),
            ),
            Dequeued::RemoveFailed => Toast::new(
                Severity::Error,
                format!("{label} left the queue, but git could not remove its worktree: {path}"),
            ),
        };
        self.push_toast(toast, std::time::Instant::now());
        Some(true)
    }
}
