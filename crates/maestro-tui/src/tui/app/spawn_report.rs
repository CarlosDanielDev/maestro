//! #1453: one report for a failed `ManagedSession::spawn`.

use super::App;
use crate::tui::activity_log::LogLevel;

impl App {
    /// A spawn error (the session state machine refusing `Spawning`, or
    /// any other) is never silent. The ERR entry shows in the Activity
    /// pane, and off the log screens it raises an error toast (#1263).
    pub(crate) fn report_spawn_failure(&mut self, label: String, err: &anyhow::Error) {
        tracing::warn!(session = %label, error = %err, "spawn failed");
        self.activity_log
            .push_simple(label, format!("Spawn failed: {err}"), LogLevel::Error);
    }
}

#[cfg(test)]
#[path = "spawn_state_tests.rs"]
mod spawn_state_tests;
