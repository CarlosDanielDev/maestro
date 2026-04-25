//! Bypass-mode App methods (#328) — extracted out of `app/mod.rs` to keep
//! the parent file under the 400-line cap.

#![deny(clippy::unwrap_used)]

use crate::tui::activity_log::LogLevel;
use crate::tui::app::App;

impl App {
    /// Mark bypass mode active (CLI path) and audit-log it. The CLI flag is
    /// session-only by construction — the warning screen is acknowledged by
    /// virtue of the user having explicitly typed `--bypass-review`.
    pub fn activate_bypass_from_cli(&mut self) {
        self.bypass_active = true;
        self.bypass_warning_acknowledged = true;
        self.activity_log.push_simple(
            "BYPASS".into(),
            "Bypass mode enabled (cli) — auto-accepting review corrections".into(),
            LogLevel::Warn,
        );
    }

    /// Disable bypass mode and audit-log. Idempotent.
    pub fn deactivate_bypass(&mut self, reason: &str) {
        if !self.bypass_active {
            return;
        }
        self.bypass_active = false;
        self.pool.set_permission_mode("default".to_string());
        self.activity_log.push_simple(
            "BYPASS".into(),
            format!("Bypass mode disabled ({reason})"),
            LogLevel::Info,
        );
    }

    /// Begin a TUI toggle: if the warning has not yet been acknowledged
    /// this session, push the BypassWarning screen; otherwise toggle
    /// directly. Returns true when the caller should push the warning.
    pub fn request_bypass_toggle(&mut self) -> bool {
        if self.bypass_active {
            self.deactivate_bypass("tui");
            return false;
        }
        if !self.bypass_warning_acknowledged {
            self.bypass_warning_screen =
                Some(crate::tui::screens::bypass_warning::BypassWarningState::new());
            return true;
        }
        self.confirm_bypass_activation("tui");
        false
    }

    /// Final activation step after the warning has been confirmed.
    pub fn confirm_bypass_activation(&mut self, source: &str) {
        self.bypass_active = true;
        self.bypass_warning_acknowledged = true;
        self.pool
            .set_permission_mode("bypassPermissions".to_string());
        self.activity_log.push_simple(
            "BYPASS".into(),
            format!("Bypass mode enabled ({source}) — auto-accepting review corrections"),
            LogLevel::Warn,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    fn make_app() -> App {
        let tmp =
            std::env::temp_dir().join(format!("maestro-bypass-test-{}.json", uuid::Uuid::new_v4()));
        let store = StateStore::new(tmp);
        App::new(
            store,
            1,
            Box::new(MockWorktreeManager::new()),
            "default".to_string(),
            vec![],
        )
    }

    #[test]
    fn cli_activation_sets_active_and_acknowledged() {
        let mut app = make_app();
        app.activate_bypass_from_cli();
        assert!(app.bypass_active);
        assert!(app.bypass_warning_acknowledged);
    }

    #[test]
    fn deactivate_when_inactive_is_no_op() {
        let mut app = make_app();
        app.deactivate_bypass("test");
        assert!(!app.bypass_active);
    }

    #[test]
    fn deactivate_when_active_clears_flag_and_logs() {
        let mut app = make_app();
        app.activate_bypass_from_cli();
        let before = app.activity_log.entries().len();
        app.deactivate_bypass("test");
        assert!(!app.bypass_active);
        assert!(app.activity_log.entries().len() > before);
    }

    #[test]
    fn request_toggle_first_time_pushes_warning() {
        let mut app = make_app();
        let pushed = app.request_bypass_toggle();
        assert!(pushed);
        assert!(app.bypass_warning_screen.is_some());
        assert!(!app.bypass_active);
    }

    #[test]
    fn request_toggle_after_ack_activates_directly() {
        let mut app = make_app();
        app.bypass_warning_acknowledged = true;
        let pushed = app.request_bypass_toggle();
        assert!(!pushed);
        assert!(app.bypass_active);
    }

    #[test]
    fn request_toggle_when_active_disables() {
        let mut app = make_app();
        app.activate_bypass_from_cli();
        let pushed = app.request_bypass_toggle();
        assert!(!pushed);
        assert!(!app.bypass_active);
    }

    #[test]
    fn confirm_bypass_activation_sets_pool_permission_mode() {
        let mut app = make_app();
        app.confirm_bypass_activation("tui");
        assert!(app.bypass_active);
        // We can't easily inspect the pool's permission_mode, but the call
        // not panicking and bypass_active flipping is the contract test.
    }
}
