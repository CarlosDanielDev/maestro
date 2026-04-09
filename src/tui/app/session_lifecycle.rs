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
}
