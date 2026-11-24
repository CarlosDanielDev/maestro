//! Pause and resume one agent (#1292): the file-conflict toast's `p`
//! (`agent_graph::conflict_toast`) and its `⌃Z` undo.

use super::App;
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;

impl App {
    /// SIGSTOP one Running agent and record it Paused. `false` when it is
    /// not Running or has no process (it is never shown Paused then).
    pub(crate) fn pause_session(&mut self, id: uuid::Uuid) -> bool {
        let Some(managed) = self.pool.get_active_mut(id) else {
            return false;
        };
        if managed.session.status != SessionStatus::Running || managed.session.pid.is_none() {
            return false;
        }
        #[cfg(unix)]
        {
            let _ = managed.pause();
            let paused = (managed.session)
                .transition_to(SessionStatus::Paused, TransitionReason::UserPause)
                .is_ok();
            self.sync_state();
            paused
        }
        #[cfg(not(unix))]
        false
    }

    /// `⌃Z` after `p`: SIGCONT and record it Running again.
    pub(crate) fn resume_session(&mut self, id: uuid::Uuid) {
        let Some(managed) = self.pool.get_active_mut(id) else {
            return;
        };
        if managed.session.status != SessionStatus::Paused {
            return;
        }
        #[cfg(unix)]
        {
            let _ = managed.resume();
            let _ = (managed.session)
                .transition_to(SessionStatus::Running, TransitionReason::UserResume);
        }
        self.sync_state();
    }
}
