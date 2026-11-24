//! Pause and resume one agent (#1292): the file-conflict toast's `⌃P`
//! (`agent_graph::conflict_toast`) and its `⌃Z` undo.

use super::App;
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;

impl App {
    /// SIGSTOP one Running agent and record it Paused. `Err` says why
    /// nothing was paused: it is not running, has no process, or the
    /// signal failed. It is never recorded Paused then.
    pub(crate) fn pause_session(&mut self, id: uuid::Uuid) -> Result<(), String> {
        let Some(managed) = self.pool.get_active_mut(id) else {
            return Err("it is not running".into());
        };
        let status = managed.session.status;
        if status != SessionStatus::Running {
            return Err(format!(
                "it is {}",
                status.label().to_lowercase().replace('_', " ")
            ));
        }
        if managed.session.pid.is_none() {
            return Err("it has no process to pause".into());
        }
        #[cfg(unix)]
        {
            managed.pause().map_err(|e| e.to_string())?;
            (managed.session)
                .transition_to(SessionStatus::Paused, TransitionReason::UserPause)
                .map_err(|e| e.to_string())?;
            self.sync_state();
            Ok(())
        }
        #[cfg(not(unix))]
        Err("pausing needs a unix signal".into())
    }

    /// `⌃Z` after `⌃P`: SIGCONT and record it Running again.
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
