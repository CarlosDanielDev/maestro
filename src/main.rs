mod budget;
mod changelog;
mod cli;
mod commands;
mod config;
mod continuous;
mod doctor;
mod flags;
mod gates;
mod git;
mod icon_mode;
mod icons;
mod models;
mod modes;
mod notifications;
mod plugins;
mod prompts;
mod provider;
mod review;
mod session;
mod state;
mod tui;
mod updater;
mod util;
mod work;

mod adapt;
mod mascot;
mod sanitize;
mod system;
mod turboquant;

#[cfg(test)]
mod integration_tests;

use clap::Parser;
use cli::{Cli, Commands, PrdSourceArg};
use commands::*;

impl From<PrdSourceArg> for adapt::prd_source::PrdSource {
    fn from(arg: PrdSourceArg) -> Self {
        match arg {
            PrdSourceArg::Local => Self::Local,
            PrdSourceArg::Github => Self::Github,
            PrdSourceArg::Azure => Self::Azure,
            PrdSourceArg::Both => Self::Both,
        }
    }
}

/// Cross-platform log writer that falls back to `io::sink()` if the log file
/// cannot be opened (avoids the `/dev/null` panic on non-Unix platforms).
enum LogWriter {
    File(std::fs::File),
    Sink(std::io::Sink),
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::File(f) => f.write(buf),
            Self::Sink(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::File(f) => f.flush(),
            Self::Sink(s) => s.flush(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("maestro=debug")
        .with_writer(|| -> LogWriter {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("maestro.log")
            {
                Ok(f) => LogWriter::File(f),
                Err(e) => {
                    eprintln!("Warning: cannot open maestro.log: {e}");
                    LogWriter::Sink(std::io::sink())
                }
            }
        })
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => cmd_init(),
        Some(Commands::Clean { dry_run }) => cmd_clean(dry_run),
        Some(Commands::Logs { session, export }) => cmd_logs(session, export),
        Some(Commands::Resume { session }) => cmd_resume(session).await,
        Some(Commands::TestSlack) => cmd_test_slack().await,
        Some(Commands::Completions { shell }) => cli::cmd_completions(shell),
        Some(Commands::Mangen { out_dir }) => cli::cmd_mangen(&out_dir),
        Some(Commands::Doctor) => cmd_doctor(),
        Some(Commands::Adapt {
            path,
            dry_run,
            no_issues,
            scan_only,
            model,
            source,
        }) => {
            adapt::cmd_adapt(adapt::AdaptConfig {
                path,
                dry_run,
                no_issues,
                scan_only,
                model,
                prd_source: source.into(),
            })
            .await
        }
        Some(Commands::Prd {
            path,
            model,
            force,
            source,
        }) => {
            adapt::cmd_prd(adapt::PrdConfig {
                path,
                model,
                force,
                source: source.into(),
            })
            .await
        }
        Some(Commands::Sanitize {
            path,
            output,
            severity,
            skip_ai,
            model,
        }) => {
            let output_fmt = match output {
                cli::SanitizeOutputFormat::Text => sanitize::OutputFormat::Text,
                cli::SanitizeOutputFormat::Json => sanitize::OutputFormat::Json,
                cli::SanitizeOutputFormat::Markdown => sanitize::OutputFormat::Markdown,
            };
            let sev = match severity {
                cli::SanitizeSeverityFilter::Critical => sanitize::Severity::Critical,
                cli::SanitizeSeverityFilter::Warning => sanitize::Severity::Warning,
                cli::SanitizeSeverityFilter::Info => sanitize::Severity::Info,
            };
            sanitize::cmd_sanitize(sanitize::SanitizeConfig {
                path,
                output: output_fmt,
                severity: sev,
                skip_ai,
                model,
            })
            .await
        }
        Some(Commands::TurboQuant { action }) => match action {
            cli::TurboQuantAction::Benchmark {
                dim,
                count,
                bits,
                output,
            } => cmd_turboquant_benchmark(dim, count, bits, output),
        },
        Some(Commands::Status) => cmd_status(),
        Some(Commands::Cost) => cmd_cost(),
        Some(Commands::Queue) => cmd_queue().await,
        Some(Commands::Add { issue_number }) => cmd_add(issue_number).await,
        Some(Commands::Run {
            prompt,
            issue,
            milestone,
            model,
            mode,
            max_concurrent,
            resume,
            skip_doctor,
            images,
            once,
            continuous,
            enable_flags,
            disable_flags,
            no_splash,
        }) => {
            cmd_run(
                prompt,
                issue,
                milestone,
                model,
                mode,
                max_concurrent,
                resume,
                skip_doctor,
                images,
                once,
                continuous,
                enable_flags,
                disable_flags,
                no_splash,
            )
            .await
        }
        None => cmd_dashboard().await,
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::setup::setup_app_from_config;
    use crate::config::Config;
    use crate::session::types::Session;
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    fn make_store() -> StateStore {
        let tmp = std::env::temp_dir().join(format!("maestro-test-{}.json", uuid::Uuid::new_v4()));
        StateStore::new(tmp)
    }

    fn make_worktree_mgr() -> Box<dyn crate::session::worktree::WorktreeManager + Send> {
        Box::new(MockWorktreeManager::new())
    }

    fn minimal_config() -> Config {
        toml::from_str(
            r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            "#,
        )
        .unwrap()
    }

    fn config_with_sessions(extra: &str) -> Config {
        let toml_str = format!(
            r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            {}
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            "#,
            extra
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn budget_enforcer_is_wired_from_config() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(app.budget_enforcer.is_some());
    }

    #[test]
    fn model_router_is_wired_from_config() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(app.model_router.is_some());
    }

    #[test]
    fn configure_sets_fork_policy() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(
            app.fork_policy.is_some(),
            "configure() must set fork_policy"
        );
    }

    #[test]
    fn config_is_stored_on_app() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(app.config.is_some());
    }

    #[test]
    fn plugin_runner_is_none_when_no_plugins() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(app.plugin_runner.is_none());
    }

    #[test]
    fn plugin_runner_is_some_when_plugins_configured() {
        let config: Config = toml::from_str(
            r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            [[plugins]]
            name = "test-hook"
            on = "session_completed"
            run = "echo done"
            "#,
        )
        .unwrap();
        let app = setup_app_from_config(config, make_store(), make_worktree_mgr(), None);
        assert!(app.plugin_runner.is_some());
    }

    #[test]
    fn permission_mode_from_config_is_preserved() {
        let config = config_with_sessions(r#"permission_mode = "acceptEdits""#);
        let app = setup_app_from_config(config, make_store(), make_worktree_mgr(), None);
        assert_eq!(
            app.config.as_ref().unwrap().sessions.permission_mode,
            "acceptEdits"
        );
    }

    #[test]
    fn default_permission_mode_is_bypass_permissions() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert_eq!(
            app.config.as_ref().unwrap().sessions.permission_mode,
            "bypassPermissions"
        );
    }

    #[test]
    fn allowed_tools_from_config_are_preserved() {
        let config = config_with_sessions(r#"allowed_tools = ["Read", "Write"]"#);
        let app = setup_app_from_config(config, make_store(), make_worktree_mgr(), None);
        assert_eq!(
            app.config.as_ref().unwrap().sessions.allowed_tools,
            vec!["Read", "Write"]
        );
    }

    #[test]
    fn max_concurrent_override_takes_priority() {
        let config = config_with_sessions("max_concurrent = 5");
        let mut app = setup_app_from_config(config, make_store(), make_worktree_mgr(), Some(1));
        for i in 0..3 {
            app.pool.enqueue(Session::new(
                format!("prompt {i}"),
                "opus".into(),
                "orchestrator".into(),
                None,
            ));
        }
        app.pool.try_promote();
        assert_eq!(app.pool.active_count(), 1);
    }

    #[test]
    fn max_concurrent_from_config_when_no_override() {
        let config = config_with_sessions("max_concurrent = 2");
        let mut app = setup_app_from_config(config, make_store(), make_worktree_mgr(), None);
        for i in 0..3 {
            app.pool.enqueue(Session::new(
                format!("prompt {i}"),
                "opus".into(),
                "orchestrator".into(),
                None,
            ));
        }
        app.pool.try_promote();
        assert_eq!(app.pool.active_count(), 2);
    }

    #[test]
    fn dashboard_does_not_hardcode_permission_mode() {
        let config = config_with_sessions(r#"permission_mode = "plan""#);
        let app = setup_app_from_config(config, make_store(), make_worktree_mgr(), None);
        assert_eq!(
            app.config.as_ref().unwrap().sessions.permission_mode,
            "plan",
            "cmd_dashboard must not override permission_mode with a hardcoded value"
        );
    }

    #[test]
    fn app_once_mode_field_defaults_to_false() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(
            !app.once_mode,
            "App built from config must have once_mode = false"
        );
    }

    #[test]
    fn app_once_mode_field_is_settable() {
        let mut app =
            setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        app.once_mode = true;
        assert!(app.once_mode, "once_mode must be directly settable");
    }

    #[test]
    fn feature_flags_are_assignable_to_app() {
        let mut app =
            setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        let flags = crate::flags::store::FeatureFlags::new(
            std::collections::HashMap::new(),
            vec!["ci_auto_fix".to_string()],
            vec![],
        );
        app.flags = flags;
        assert!(
            app.flags.is_enabled(crate::flags::Flag::CiAutoFix),
            "app.flags must reflect assigned FeatureFlags"
        );
    }

    #[test]
    fn feature_flags_cli_disable_overrides_config_enable() {
        let mut config_entries = std::collections::HashMap::new();
        config_entries.insert("ci_auto_fix".to_string(), true);
        let flags = crate::flags::store::FeatureFlags::new(
            config_entries,
            vec![],
            vec!["ci_auto_fix".to_string()],
        );
        assert!(
            !flags.is_enabled(crate::flags::Flag::CiAutoFix),
            "CLI --disable-flag must override config enable"
        );
    }

    #[test]
    fn feature_flags_default_on_app_matches_flag_defaults() {
        let app = setup_app_from_config(minimal_config(), make_store(), make_worktree_mgr(), None);
        assert!(
            app.flags.is_enabled(crate::flags::Flag::ContinuousMode),
            "ContinuousMode must default to true"
        );
        assert!(
            app.flags.is_enabled(crate::flags::Flag::AutoFork),
            "AutoFork must default to true"
        );
        assert!(
            !app.flags.is_enabled(crate::flags::Flag::CiAutoFix),
            "CiAutoFix must default to false"
        );
    }
}
