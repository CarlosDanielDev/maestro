use super::App;
use super::helpers::session_label;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::types::Session;
use crate::tui::activity_log::LogLevel;

impl App {
    pub async fn add_session(&mut self, session: Session) -> anyhow::Result<()> {
        let label = session_label(&session);
        self.activity_log
            .push_simple(label.clone(), "Enqueuing session...".into(), LogLevel::Info);

        // Reset dismissed flag so completion summary can trigger for new sessions
        self.completion_summary_dismissed = false;

        self.pool.enqueue(session);

        // Try to promote and spawn
        let promoted_ids = self.pool.try_promote();
        let tx = self.pool.event_tx();
        for id in promoted_ids {
            if let Some(managed) = self.pool.get_active_mut(id) {
                let session_label = session_label(&managed.session);
                self.activity_log.push_simple(
                    session_label.clone(),
                    "Spawning session...".into(),
                    LogLevel::Info,
                );
                if let Err(e) = managed.spawn(tx.clone()).await {
                    self.activity_log.push_simple(
                        session_label,
                        format!("Spawn failed: {}", e),
                        LogLevel::Error,
                    );
                } else {
                    self.activity_log.push_simple(
                        session_label,
                        "Session started".into(),
                        LogLevel::Info,
                    );
                    // Fire session_started plugin hook
                    let ctx = HookContext::new().with_session(
                        &managed.session.id.to_string(),
                        managed.session.issue_number,
                    );
                    self.fire_plugin_hook(HookPoint::SessionStarted, ctx).await;
                }
            }
        }

        self.sync_state();
        Ok(())
    }

    pub fn pause_all(&self) {
        self.pool.pause_all();
    }

    /// Resume all paused sessions.
    #[cfg(unix)]
    pub fn resume_all(&self) {
        self.pool.resume_all();
    }

    /// Kill all sessions.
    pub async fn kill_all(&mut self) {
        self.pool.kill_all().await;
        self.sync_state();
    }

    /// Check if all sessions are done.
    pub fn all_done(&self) -> bool {
        self.pool.all_done()
    }

    pub fn active_count(&self) -> usize {
        self.pool.active_count()
    }

    /// Dismiss a single completed/terminal session from the TUI.
    pub fn dismiss_session(&mut self, session_id: uuid::Uuid) {
        let label = self
            .pool
            .get_session(session_id)
            .map(session_label)
            .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
        if self.pool.dismiss_session(session_id) {
            self.session_ui_state.remove(&session_id);
            self.tool_start_times.remove(&session_id);
            self.activity_log
                .push_simple(label, "Session dismissed".into(), LogLevel::Info);
            self.sync_state();
        }
    }

    /// Dismiss all completed sessions from the TUI.
    pub fn dismiss_all_completed(&mut self) {
        let count = self.pool.dismiss_all_completed();
        if count > 0 {
            self.session_ui_state
                .retain(|id, _| self.pool.get_session(*id).is_some());
            self.tool_start_times
                .retain(|id, _| self.pool.get_session(*id).is_some());
            self.activity_log.push_simple(
                "SYSTEM".into(),
                format!("Dismissed {} completed session(s)", count),
                LogLevel::Info,
            );
            self.sync_state();
        }
    }

    /// Kill a single session (called after user confirms).
    pub async fn kill_selected_session(&mut self, session_id: uuid::Uuid) {
        let label = self
            .pool
            .get_session(session_id)
            .map(session_label)
            .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
        match self.pool.kill_session(session_id).await {
            Ok(true) => {
                self.activity_log.push_simple(
                    label,
                    "Session killed by user".into(),
                    LogLevel::Warn,
                );
                self.sync_state();
            }
            Ok(false) => {
                self.activity_log.push_simple(
                    label,
                    "Session not found or already finished".into(),
                    LogLevel::Warn,
                );
            }
            Err(e) => {
                self.activity_log.push_simple(
                    label,
                    format!("Kill failed: {}", e),
                    LogLevel::Error,
                );
            }
        }
    }

    /// Toggle the summary popup for a completed session.
    pub fn toggle_session_summary(&mut self, session_id: uuid::Uuid) {
        let entry = self.session_ui_state.entry(session_id).or_default();
        entry.show_summary_popup = !entry.show_summary_popup;
    }
}
