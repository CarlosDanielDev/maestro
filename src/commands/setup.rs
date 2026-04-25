use crate::config::{LoadedConfig, NotificationsConfig};
use crate::notifications::dispatcher::NotificationDispatcher;
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
    let LoadedConfig { config, path } = loaded;
    let max_concurrent = max_concurrent_override.unwrap_or(config.sessions.max_concurrent);

    // Bypass overrides the configured permission mode for this session only.
    let permission_mode = if bypass_review {
        "bypassPermissions".to_string()
    } else {
        config.sessions.permission_mode.clone()
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
    //   - Config (`[sessions].permission_mode = "bypassPermissions"`)
    //   - TUI toggle (Ctrl+B at runtime)
    if bypass_review || permission_mode == "bypassPermissions" {
        app.activate_bypass_from_cli();
    }

    app.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(
        config.budget.per_session_usd,
        config.budget.total_usd,
        config.budget.alert_threshold_pct,
    ));

    app.model_router = Some(crate::models::ModelRouter::new(
        config.models.routing.clone(),
        config.sessions.default_model.clone(),
    ));

    app.notifications = build_notification_dispatcher(&config.notifications);

    if !config.plugins.is_empty() {
        app.plugin_runner = Some(crate::plugins::runner::PluginRunner::new(
            config.plugins.clone(),
            DEFAULT_PLUGIN_TIMEOUT_SECS,
        ));
    }

    app.configure(config);
    app.set_config_path(path);
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

pub fn build_notification_dispatcher(cfg: &NotificationsConfig) -> NotificationDispatcher {
    let mut dispatcher = NotificationDispatcher::new(cfg.desktop);
    if cfg.slack {
        if let Some(ref url) = cfg.slack_webhook_url {
            dispatcher = dispatcher.with_slack(url.clone(), cfg.slack_rate_limit_per_min);
            tracing::info!("Slack notifications enabled");
        } else {
            tracing::warn!("notifications.slack = true but no slack_webhook_url configured");
        }
    }
    dispatcher
}
