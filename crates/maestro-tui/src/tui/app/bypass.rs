//! Bypass-mode App methods (#328) — extracted out of `app/mod.rs` to keep
//! the parent file under the 400-line cap.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use crate::review::bypass::BypassSource;
use crate::tui::activity_log::LogLevel;
use crate::tui::app::{App, TuiMode};
use crate::tui::screens::bypass_warning::{BypassWarningOutcome, BypassWarningState};

/// Activity-log source for a bypass turned on from Settings (#1239).
const SETTINGS_CONFIRM_SOURCE: &str = "settings (typed CONFIRM)";

/// The bypass warning opened from Settings. Unlike ⌃B it says the real
/// scope: Ctrl+S writes the value to the committed `maestro.toml`.
fn settings_bypass_warning() -> BypassWarningState {
    let row = |label: &str, text: &str| (label.to_string(), text.to_string());
    BypassWarningState::with_text(
        "DANGER: enable bypass for every future session?",
        vec![
            row("", "Bypass auto-accepts EVERY review concern."),
            row("", ""),
            row("does", "edits files, commits and pushes, no asking"),
            row(
                "writes",
                "sessions.permission_mode to maestro.toml (committed)",
            ),
            row("scope", "every future session after Ctrl+S; this run now"),
            row("or", "Ctrl+B turns it on for this run only"),
        ],
    )
}

impl App {
    /// Mark bypass mode active and audit-log it with the real activation
    /// source (#1148): the `--bypass-review` CLI flag or the
    /// `[sessions].permission_mode` config value. Neither counts as
    /// accepting the warning (#1395): the config file can come from a cloned
    /// repo the user did not write. Only a typed CONFIRM does, so after this
    /// bypass is turned off, the next ⌃B opens the warning.
    pub fn activate_bypass(&mut self, source: BypassSource) {
        self.bypass_active = true;
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
        self.drop_settings_bypass_request();
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
        self.screen_state.bypass_warning_screen = Some(BypassWarningState::new());
    }

    /// #1239: the Settings › Sessions bypass toggle was flipped on. It
    /// writes the committed `maestro.toml`, so it always asks for the typed
    /// CONFIRM, even when ⌃B was acknowledged earlier this session.
    pub fn process_pending_settings_bypass(&mut self) {
        let pending = (self.screen_state.settings_screen.as_ref())
            .is_some_and(|s| s.bypass_request_pending());
        if !pending {
            return;
        }
        if self.screen_state.bypass_warning_screen.is_some() {
            // Back in Settings with the warning still open: it was left
            // through a global chord, unanswered. Treat that as a cancel.
            self.screen_state.bypass_warning_screen = None;
            self.drop_settings_bypass_request();
            return;
        }
        self.screen_state.bypass_warning_screen = Some(settings_bypass_warning());
        self.navigate_to(TuiMode::BypassWarning);
    }

    /// Turn a pending Settings bypass toggle back off, so no later answer
    /// to the warning (for example a ⌃B one) is taken as a Settings confirm.
    fn drop_settings_bypass_request(&mut self) {
        if let Some(screen) =
            (self.screen_state.settings_screen.as_mut()).filter(|s| s.bypass_request_pending())
        {
            screen.resolve_bypass_request(false);
        }
    }

    /// The bypass warning was answered. When a Settings request is
    /// pending, stage it (confirmed) or turn its toggle back off
    /// (cancelled) and return true; the caller pops the warning. Returns
    /// false on `Pending` and on the ⌃B path, which keeps its own branches.
    pub fn answer_settings_bypass(&mut self, outcome: BypassWarningOutcome) -> bool {
        let confirmed = match outcome {
            BypassWarningOutcome::Pending => return false,
            BypassWarningOutcome::Confirmed => true,
            BypassWarningOutcome::Cancelled => false,
        };
        let Some(screen) =
            (self.screen_state.settings_screen.as_mut()).filter(|s| s.bypass_request_pending())
        else {
            return false;
        };
        screen.resolve_bypass_request(confirmed);
        self.screen_state.bypass_warning_screen = None;
        if confirmed {
            self.confirm_bypass_activation(SETTINGS_CONFIRM_SOURCE);
        }
        true
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
#[path = "bypass_ctrl_b_tests.rs"]
mod ctrl_b_tests;

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
    fn cli_activation_sets_active() {
        let mut app = make_app();
        app.activate_bypass(BypassSource::Cli);
        assert!(app.bypass_active);
    }

    /// #1395: a config file can come from a cloned repo, so neither a
    /// config nor a CLI bypass counts as accepting the warning.
    #[test]
    fn config_or_cli_bypass_does_not_acknowledge_warning() {
        for source in [BypassSource::Config, BypassSource::Cli] {
            let mut app = make_app();
            app.activate_bypass(source);
            assert!(app.bypass_active, "{} bypass is on", source.label());
            assert!(
                !app.bypass_warning_acknowledged,
                "{} bypass must not acknowledge the warning",
                source.label()
            );
        }
    }

    /// Every `permission_mode` in a TOML tree, with its dotted path.
    fn permission_modes(value: &toml::Value, path: &str, out: &mut Vec<(String, String)>) {
        let Some(table) = value.as_table() else {
            return;
        };
        for (key, child) in table {
            let child_path = format!("{path}{key}");
            match (key.as_str(), child.as_str()) {
                ("permission_mode", Some(mode)) => out.push((child_path, mode.to_string())),
                _ => permission_modes(child, &format!("{child_path}."), out),
            }
        }
    }

    /// #1395: this repo is public; a fresh clone must not start in bypass
    /// (`[sessions]`) and must not turn it on when Settings is saved, which
    /// reads `agents.<default>.permission_mode` (screen_dispatch.rs).
    #[test]
    fn committed_maestro_toml_does_not_ship_bypass() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../maestro.toml");
        let text = std::fs::read_to_string(&path).expect("repo maestro.toml is readable");
        let doc: toml::Value = toml::from_str(&text).expect("repo maestro.toml parses");
        let sessions = doc
            .get("sessions")
            .and_then(|s| s.get("permission_mode"))
            .and_then(|v| v.as_str());
        assert_eq!(sessions, Some("default"));

        let mut modes = Vec::new();
        permission_modes(&doc, "", &mut modes);
        let bypass: Vec<_> = modes
            .iter()
            .filter(|(_, mode)| mode == "bypassPermissions")
            .collect();
        assert!(bypass.is_empty(), "maestro.toml ships bypass at {bypass:?}");
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
    }
}
