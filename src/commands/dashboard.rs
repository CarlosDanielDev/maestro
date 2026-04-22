use crate::commands::setup::{DEFAULT_MAX_CONCURRENT, setup_app_from_config, startup_cleanup};
use crate::config::Config;
use crate::provider::github::client::GhCliClient;
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::App;

pub async fn cmd_dashboard() -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;
    startup_cleanup(&repo_root);

    let store = StateStore::new(StateStore::default_path());
    let state = store.load().unwrap_or_default();

    let has_incomplete = state.sessions.iter().any(|s| {
        matches!(
            s.status,
            crate::session::types::SessionStatus::Running
                | crate::session::types::SessionStatus::Spawning
                | crate::session::types::SessionStatus::Queued
                | crate::session::types::SessionStatus::Stalled
                | crate::session::types::SessionStatus::Retrying
        )
    });

    if has_incomplete {
        eprintln!(
            "Found incomplete sessions from previous run. Use `maestro resume` to continue them."
        );
    }

    let loaded = Config::find_and_load_with_path().ok();
    let config = loaded.as_ref().map(|l| l.config.clone());

    let repo_name = config
        .as_ref()
        .map(|c| c.project.repo.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            std::process::Command::new("git")
                .args(["remote", "get-url", "origin"])
                .output()
                .ok()
                .and_then(|o| {
                    let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    url.rsplit('/')
                        .next()
                        .map(|s| s.trim_end_matches(".git").to_string())
                })
                .unwrap_or_else(|| "unknown".to_string())
        });

    let branch = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let config_clone = config.clone();
    let (doctor_warnings, username, gh_auth_ok) = tokio::task::spawn_blocking(move || {
        let report = crate::doctor::run_all_checks(config_clone.as_ref());
        let warnings: Vec<String> = report
            .failed_checks()
            .iter()
            .map(|c| format!("{}: {}", c.name, c.message))
            .collect();

        let username = report
            .checks
            .iter()
            .find(|c| c.name == "gh auth" && c.passed)
            .and_then(|c| {
                c.message
                    .strip_prefix("authenticated as @")
                    .map(|rest| rest.split(',').next().unwrap_or(rest).to_string())
            });

        let gh_auth_ok = report
            .checks
            .iter()
            .find(|c| c.name == "gh auth")
            .map(|c| c.passed)
            .unwrap_or(true);

        (warnings, username, gh_auth_ok)
    })
    .await
    .unwrap_or_default();

    let project_info = crate::tui::screens::home::ProjectInfo {
        repo: repo_name,
        branch,
        username,
    };

    let recent_sessions: Vec<crate::tui::screens::home::SessionSummary> = state
        .sessions
        .iter()
        .rev()
        .take(10)
        .map(|s| crate::tui::screens::home::SessionSummary {
            issue_number: s.issue_number.unwrap_or(0),
            title: s.last_message.clone(),
            status: s.status.label().to_string(),
            cost_usd: s.cost_usd,
        })
        .collect();

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));

    let mut app = if let Some(lc) = loaded {
        let mut app = setup_app_from_config(lc, store, worktree_mgr, None);
        app.github_client = Some(Box::new(GhCliClient::new()));
        app
    } else {
        App::new(
            store,
            DEFAULT_MAX_CONCURRENT,
            worktree_mgr,
            "bypassPermissions".into(),
            Vec::new(),
        )
    };

    app.gh_auth_ok = gh_auth_ok;

    app.home_screen = Some(crate::tui::screens::HomeScreen::new(
        project_info,
        recent_sessions,
        doctor_warnings,
    ));
    app.tui_mode = crate::tui::app::TuiMode::Dashboard;

    app.pending_commands
        .push(crate::tui::app::TuiCommand::FetchSuggestionData);

    crate::tui::run(app).await
}
