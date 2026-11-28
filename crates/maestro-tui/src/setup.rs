use crate::config::LoadedConfig;
use crate::notifications::dispatcher::build_notification_dispatcher;
use crate::review::bypass::BypassSource;
use crate::state::store::StateStore;
use crate::tui::app::App;
use crate::tui::screens::sanitize_for_terminal;
use std::io::Write;
use std::path::Path;

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
    setup_app_from_config_with_bypass(
        loaded,
        store,
        worktree_mgr,
        max_concurrent_override,
        false,
        None,
    )
}

/// Variant of `setup_app_from_config` that also threads the bypass-review
/// CLI flag (#328). When `bypass_review` is true the session pool is
/// constructed with `bypassPermissions`, the App's `bypass_active` flag is
/// set, and an audit-log entry is recorded. `agent_id` is the agent the
/// run resolved (`None` = `agents.default`), see [`setup_app_for_agent`].
pub fn setup_app_from_config_with_bypass(
    loaded: LoadedConfig,
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
    max_concurrent_override: Option<usize>,
    bypass_review: bool,
    agent_id: Option<&str>,
) -> App {
    let workspace_trusted = matches!(
        crate::config::trust::evaluate_with_default_store(&loaded.path, &loaded.config),
        crate::config::trust::TrustDecision::Trusted,
    );
    setup_app_for_agent(
        loaded,
        store,
        worktree_mgr,
        max_concurrent_override,
        bypass_review,
        workspace_trusted,
        agent_id,
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
    setup_app_for_agent(
        loaded,
        store,
        worktree_mgr,
        max_concurrent_override,
        bypass_review,
        workspace_trusted,
        None,
    )
}

/// [`setup_app_from_config_with_trust`] for the agent sessions will run
/// (#1411). `maestro run` passes the agent it resolved, which can differ
/// from `agents.default`; `None` means the default agent. The pool mode
/// and the ▲ BYPASS chip are set here, together, from one value.
pub fn setup_app_for_agent(
    loaded: LoadedConfig,
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
    max_concurrent_override: Option<usize>,
    bypass_review: bool,
    workspace_trusted: bool,
    agent_id: Option<&str>,
) -> App {
    let LoadedConfig { config, path } = loaded;
    let max_concurrent = max_concurrent_override.unwrap_or(config.sessions.max_concurrent);

    // Bypass overrides the configured permission mode for this session only.
    // Otherwise use the mode sessions really get (the agent's own value
    // first, then `[sessions]`), so the ▲ BYPASS chip always matches it
    // (#1395, #1411). The log names where bypass came from.
    let agent_id = agent_id.unwrap_or(&config.agents.default);
    let agent_mode = config.agent_permission_mode(agent_id);
    let (permission_mode, bypass_source) = if bypass_review {
        (
            "bypassPermissions".to_string(),
            BypassSource::Cli.label().to_string(),
        )
    } else if let Some(mode) = agent_mode {
        (mode, format!("agent {agent_id}"))
    } else {
        (
            config.sessions.permission_mode.clone(),
            BypassSource::Config.label().to_string(),
        )
    };

    let permission_mode_for_app = permission_mode.clone();

    let mut app = App::new(
        store,
        max_concurrent,
        worktree_mgr,
        permission_mode_for_app,
        config.sessions.allowed_tools.clone(),
    );

    // Bypass mode is activatable via these paths (#328 AC, #1411):
    //   - CLI flag (`--bypass-review`)
    //   - Config (`agents.<id>.permission_mode`, else `[sessions]`)
    //   - a mode's own `permission_mode` (see `App::note_spawn_bypass`, run before every spawn)
    //   - TUI toggle (Ctrl+B at runtime)
    if permission_mode == "bypassPermissions" {
        app.activate_bypass_from(&bypass_source);
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

/// Startup housekeeping, run just before the first screen: report each
/// untracked dir under `.maestro/worktrees/` once (never delete it, #1563)
/// and remove old session logs. `stderr` (`maestro run --once`, which
/// draws no screen) also prints the lines there.
pub fn startup_cleanup(app: &mut App, repo_root: &Path, stderr: bool) {
    let mut err = std::io::stderr();
    let err = stderr.then_some(&mut err as &mut dyn Write);
    report_startup_orphans(app, repo_root, std::time::Instant::now(), err);

    let logger = crate::session::logger::SessionLogger::new(
        crate::session::logger::SessionLogger::default_dir(),
    );
    if let Ok(removed) = logger.cleanup_old_logs(30)
        && removed > 0
    {
        tracing::info!("Cleaned {} old session logs", removed);
    }
}

/// Marker inside an untracked dir: its line was shown (#1563). Removing
/// the dir by hand removes the marker too, so a new dir with the same name
/// is shown again.
const SHOWN_MARKER: &str = ".maestro-orphan-shown";

/// Log each line of [`orphan_notices`], show it as an error toast (sticky:
/// the ⌃Q launch notice that follows must not hide it) and write it to
/// `stderr` when given.
pub(crate) fn report_startup_orphans(
    app: &mut App,
    repo_root: &Path,
    now: std::time::Instant,
    mut stderr: Option<&mut dyn Write>,
) {
    use crate::tui::ds::toast::{Severity, Toast};
    for line in orphan_notices(repo_root) {
        tracing::warn!("{line}");
        if let Some(w) = stderr.as_deref_mut() {
            let _ = writeln!(w, "warning: {}", sanitize_for_terminal(&line));
        }
        app.push_toast(Toast::new(Severity::Error, line), now);
    }
}

/// The startup lines: one per untracked dir not shown before, or the scan
/// error. An untracked dir is never removed: it may hold the only copy of
/// files no git ref has, and `git worktree remove` cannot remove it.
pub(crate) fn orphan_notices(repo_root: &Path) -> Vec<String> {
    match crate::session::cleanup::CleanupManager::new(repo_root).scan_orphans() {
        Ok(orphans) => (orphans.iter())
            .filter(|o| first_notice(&o.path))
            .map(|o| orphan_line(&o.name))
            .collect(),
        Err(e) => vec![format!("orphan worktree scan failed: {e:#}")],
    }
}

/// `true` when `dir` was not shown yet, and records it as shown. A symlink
/// (never write outside the worktree root) or a marker that cannot be
/// written is shown on every launch: never silent.
fn first_notice(dir: &Path) -> bool {
    if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
        return true;
    }
    let marker = dir.join(SHOWN_MARKER);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(e) => {
            tracing::warn!("cannot record orphan notice {}: {e}", marker.display());
            true
        }
    }
}

/// `untracked dir kept · rm -rf .maestro/worktrees/<name>`: the path and
/// the exact command, run from the repo root. Fits the 80-column toast for
/// a usual worktree name.
fn orphan_line(name: &str) -> String {
    let path = format!(".maestro/worktrees/{name}");
    let plain = |c: char| c.is_ascii_alphanumeric() || "._/-@%+=:,".contains(c);
    let path = if path.chars().all(plain) {
        path
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    };
    format!("untracked dir kept · rm -rf {path}")
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

#[cfg(test)]
#[path = "setup_orphan_tests.rs"]
pub(crate) mod orphan_tests;
