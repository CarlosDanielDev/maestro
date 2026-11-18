use crate::config::LoadedConfig;
use crate::notifications::dispatcher::build_notification_dispatcher;
use crate::review::bypass::BypassSource;
use crate::state::store::StateStore;
use crate::tui::app::App;

pub const DEFAULT_MAX_CONCURRENT: usize = 3;
pub const DEFAULT_PLUGIN_TIMEOUT_SECS: u64 = 30;

/// Build a fully-configured App from a `LoadedConfig`. The resolved path is
/// propagated into `app.config_path` so the Settings screen can save back
/// to the same file regardless of CWD at save time.
pub fn setup_app_from_config(
    loaded: LoadedConfig,
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
    max_concurrent_override: Option<usize>,
) -> App {
    setup_app_from_config_with_bypass(loaded, store, worktree_mgr, max_concurrent_override, false)
}

/// Variant of `setup_app_from_config` that also threads the bypass-review
/// CLI flag (#328). When `bypass_review` is true the session pool is
/// constructed with `bypassPermissions`, the App's `bypass_active` flag is
/// set, and an audit-log entry is recorded.
pub fn setup_app_from_config_with_bypass(
    loaded: LoadedConfig,
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
    max_concurrent_override: Option<usize>,
    bypass_review: bool,
) -> App {
    let workspace_trusted = matches!(
        crate::config::trust::evaluate_with_default_store(&loaded.path, &loaded.config),
        crate::config::trust::TrustDecision::Trusted,
    );
    setup_app_from_config_with_trust(
        loaded,
        store,
        worktree_mgr,
        max_concurrent_override,
        bypass_review,
        workspace_trusted,
    )
}

/// [`setup_app_from_config`] with an explicit workspace trust decision
/// (#1045). When `workspace_trusted` is false, config-supplied commands are
/// skipped: plugins never execute, executing gates fail closed, review
/// dispatch and session spawns with config commands are blocked. Production
/// callers should use [`setup_app_from_config`], which reads the decision
/// from the user's trust store.
pub fn setup_app_from_config_with_trust(
    loaded: LoadedConfig,
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
    max_concurrent_override: Option<usize>,
    bypass_review: bool,
    workspace_trusted: bool,
) -> App {
    let LoadedConfig { config, path } = loaded;
    let max_concurrent = max_concurrent_override.unwrap_or(config.sessions.max_concurrent);

    // Bypass overrides the configured permission mode for this session only.
    // Otherwise use the mode sessions really get (`agents.<default>` first,
    // then `[sessions]`), the same one a Settings save applies, so the
    // ▲ BYPASS chip always matches it (#1395).
    let permission_mode = if bypass_review {
        "bypassPermissions".to_string()
    } else {
        config.effective_default_permission_mode()
    };

    let permission_mode_for_app = permission_mode.clone();

    let mut app = App::new(
        store,
        max_concurrent,
        worktree_mgr,
        permission_mode_for_app,
        config.sessions.allowed_tools.clone(),
    );

    // Bypass mode is activatable via three paths (#328 AC):
    //   - CLI flag (`--bypass-review`)
    //   - Config (`agents.<default>.permission_mode`, else `[sessions]`)
    //   - TUI toggle (Ctrl+B at runtime)
    if bypass_review || permission_mode == "bypassPermissions" {
        let source = if bypass_review {
            BypassSource::Cli
        } else {
            BypassSource::Config
        };
        app.activate_bypass(source);
    }

    app.install_budget(&config.budget);

    app.model_router = Some(crate::models::ModelRouter::new(
        config.models.routing.clone(),
        config.sessions.default_model.clone(),
    ));

    app.notifications = build_notification_dispatcher(&config.notifications);
    // #1044: surface Slack config problems in the activity log, where users
    // look — not only in maestro.log.
    let warn = crate::tui::activity_log::LogLevel::Warn;
    if let Some(reason) = app.notifications.slack_rejection() {
        let msg = format!("Slack notifications disabled: {reason}");
        app.activity_log.push_simple("config".into(), msg, warn);
    }
    if let Some(msg) = config.notifications.webhook_leak_warning() {
        tracing::warn!("{msg}");
        app.activity_log.push_simple("config".into(), msg, warn);
    }

    if !config.plugins.is_empty() {
        app.plugin_runner = Some(crate::plugins::runner::PluginRunner::with_trust(
            config.plugins.clone(),
            DEFAULT_PLUGIN_TIMEOUT_SECS,
            workspace_trusted,
        ));
    }
    app.workspace_trusted = workspace_trusted;
    if !workspace_trusted {
        app.gate_runner = Box::new(crate::gates::UntrustedGateRunner);
    }

    app.flags = crate::flags::store::FeatureFlags::new(
        config.flags.entries.clone(),
        Vec::new(),
        Vec::new(),
    );
    app.configure(config);
    app.set_config_path(path);
    app = app.with_settings_store(Box::new(crate::settings::FsSettingsStore::new(
        std::path::PathBuf::from(".claude/settings.json"),
    )));
    app
}

/// Perform startup housekeeping: remove orphaned worktrees and old session logs.
pub fn startup_cleanup(repo_root: &std::path::Path) {
    let cleanup_mgr = crate::session::cleanup::CleanupManager::new(repo_root);
    if let Ok(orphans) = cleanup_mgr.scan_orphans()
        && !orphans.is_empty()
    {
        tracing::info!("Cleaning {} orphaned worktrees on startup", orphans.len());
        let _ = cleanup_mgr.remove_orphans(&orphans);
    }

    let logger = crate::session::logger::SessionLogger::new(
        crate::session::logger::SessionLogger::default_dir(),
    );
    if let Ok(removed) = logger.cleanup_old_logs(30)
        && removed > 0
    {
        tracing::info!("Cleaned {} old session logs", removed);
    }
}

#[cfg(test)]
mod bypass_source_tests {
    use super::*;
    use crate::session::worktree::MockWorktreeManager;
    use crate::tui::app::App;
    use tempfile::TempDir;

    fn config_with_permission_mode(mode: &str) -> crate::config::Config {
        let toml_body = format!(
            r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
permission_mode = "{mode}"
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "claude"
"#
        );
        toml::from_str(&toml_body).unwrap()
    }

    fn build_app(bypass_review: bool, mode: &str) -> (App, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = setup_app_from_config_with_trust(
            LoadedConfig {
                config: config_with_permission_mode(mode),
                path: dir.path().join("maestro.toml"),
            },
            StateStore::new(dir.path().join("state-bypass-source.json")),
            Box::new(MockWorktreeManager::new()),
            None,
            bypass_review,
            true,
        );
        (app, dir)
    }

    fn bypass_log_message(app: &App) -> String {
        app.activity_log
            .entries()
            .iter()
            .rev()
            .find(|e| e.message.contains("Bypass mode enabled"))
            .unwrap_or_else(|| panic!("no bypass activation entry logged"))
            .message
            .clone()
    }

    fn build_app_with_webhook(url: &str) -> (App, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config_with_permission_mode("default");
        config.notifications.slack = true;
        config.notifications.slack_webhook_url = Some(url.to_string());
        let app = setup_app_from_config_with_trust(
            LoadedConfig {
                config,
                path: dir.path().join("maestro.toml"),
            },
            StateStore::new(dir.path().join("state-webhook.json")),
            Box::new(MockWorktreeManager::new()),
            None,
            false,
            true,
        );
        (app, dir)
    }

    fn logged(app: &App, needle: &str) -> bool {
        app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains(needle))
    }

    #[test]
    fn webhook_in_config_warns_in_activity_log() {
        let (app, _dir) = build_app_with_webhook("https://hooks.slack.com/services/T/B/x");
        assert!(app.notifications.has_slack());
        assert!(logged(&app, "slack_webhook_url is set in maestro.toml"));
    }

    #[test]
    fn rejected_webhook_is_reported_in_activity_log() {
        let (app, _dir) = build_app_with_webhook("http://hooks.slack.com/services/T/B/x");
        assert!(!app.notifications.has_slack());
        assert!(logged(&app, "Slack webhook URL rejected"));
    }

    #[test]
    fn bypass_source_is_cli_for_flag_and_config_for_config_value() {
        let (app, _dir) = build_app(true, "default");
        let message = bypass_log_message(&app);
        assert!(
            message.contains("(cli)"),
            "flag path must log (cli), got: {message}"
        );

        let (app, _dir) = build_app(false, "bypassPermissions");
        let message = bypass_log_message(&app);
        assert!(
            message.contains("(config)"),
            "config path must log (config), got: {message}"
        );
    }
}
