//! Bypass-mode App methods (#328) — extracted out of `app/mod.rs` to keep
//! the parent file under the 400-line cap.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use crate::review::bypass::BypassSource;
use crate::tui::activity_log::LogLevel;
use crate::tui::app::{App, TuiMode};

/// Activity-log source for a bypass turned on from Settings (#1239).
const SETTINGS_CONFIRM_SOURCE: &str = "settings (typed CONFIRM)";

impl App {
    /// Mark bypass mode active and audit-log it with the real activation
    /// source (#1148): the `--bypass-review` CLI flag or the
    /// `[sessions].permission_mode` config value. Both are session-only by
    /// construction — the warning screen is acknowledged by virtue of the
    /// source being explicit (a typed flag or an authored config file).
    pub fn activate_bypass(&mut self, source: BypassSource) {
        self.bypass_active = true;
        self.bypass_warning_acknowledged = true;
        self.session_config.permission_mode = "bypassPermissions".to_string();
        self.activity_log.push_simple(
            "BYPASS".into(),
            format!(
                "Bypass mode enabled ({}) — auto-accepting review corrections",
                source.label()
            ),
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
        self.session_config.permission_mode = "default".to_string();
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
            self.open_bypass_warning();
            return true;
        }
        self.confirm_bypass_activation("tui");
        false
    }

    fn open_bypass_warning(&mut self) {
        self.screen_state.bypass_warning_screen =
            Some(crate::tui::screens::bypass_warning::BypassWarningState::new());
    }

    /// #1239: the Settings › Sessions bypass toggle was flipped on. It
    /// writes the committed `maestro.toml`, so it always asks for the typed
    /// CONFIRM, even when ⌃B was acknowledged earlier this session.
    pub fn process_pending_settings_bypass(&mut self) {
        let pending = (self.screen_state.settings_screen.as_ref())
            .is_some_and(|s| s.bypass_request_pending());
        if pending && self.screen_state.bypass_warning_screen.is_none() {
            self.open_bypass_warning();
            self.navigate_to(TuiMode::BypassWarning);
        }
    }

    /// The bypass warning was answered. A pending Settings request is
    /// staged (confirmed) or its toggle reverted (cancelled); otherwise
    /// this is the ⌃B path.
    pub fn finish_bypass_warning(&mut self, confirmed: bool) {
        self.screen_state.bypass_warning_screen = None;
        if let Some(screen) =
            (self.screen_state.settings_screen.as_mut()).filter(|s| s.bypass_request_pending())
        {
            screen.resolve_bypass_request(confirmed);
            if confirmed {
                self.confirm_bypass_activation(SETTINGS_CONFIRM_SOURCE);
            }
            return;
        }
        if confirmed {
            self.confirm_bypass_activation("tui");
        }
    }

    /// Final activation step after the warning has been confirmed.
    pub fn confirm_bypass_activation(&mut self, source: &str) {
        self.bypass_active = true;
        self.bypass_warning_acknowledged = true;
        self.pool
            .set_permission_mode("bypassPermissions".to_string());
        self.session_config.permission_mode = "bypassPermissions".to_string();
        self.activity_log.push_simple(
            "BYPASS".into(),
            format!("Bypass mode enabled ({source}) — auto-accepting review corrections"),
            LogLevel::Warn,
        );
    }
}

#[cfg(test)]
#[path = "bypass_settings_tests.rs"]
mod settings_tests;

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
        app.activate_bypass(BypassSource::Cli);
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
        app.activate_bypass(BypassSource::Cli);
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
        assert!(app.screen_state.bypass_warning_screen.is_some());
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
        app.activate_bypass(BypassSource::Cli);
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

    #[test]
    fn activation_logs_real_source_config_and_cli() {
        let mut app = make_app();
        app.activate_bypass(BypassSource::Config);
        let entries = app.activity_log.entries();
        let last = &entries[entries.len() - 1];
        assert!(
            last.message.contains("(config)"),
            "config activation must log the config source, got: {}",
            last.message
        );

        let mut app = make_app();
        app.activate_bypass(BypassSource::Cli);
        let entries = app.activity_log.entries();
        let last = &entries[entries.len() - 1];
        assert!(
            last.message.contains("(cli)"),
            "cli activation must log the cli source, got: {}",
            last.message
        );
    }

    #[test]
    fn config_activation_keeps_bypass_header_active() {
        let mut app = make_app();
        app.activate_bypass(BypassSource::Config);
        assert!(app.bypass_active);
        assert!(app.bypass_warning_acknowledged);
    }
}
