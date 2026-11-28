use crate::commands::setup::{DEFAULT_MAX_CONCURRENT, setup_app_from_config, startup_cleanup};
use crate::config::Config;
use crate::provider::create_provider;
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::App;

pub async fn cmd_dashboard() -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;

    let state_path = StateStore::default_path();
    let first_run = is_first_run(&state_path);
    let store = StateStore::new(state_path);
    let state = store.load().unwrap_or_default();

    let has_incomplete = has_incomplete(&state.sessions);

    if has_incomplete {
        eprintln!(
            "Found incomplete sessions from previous run. Use `maestro resume` to continue them."
        );
    }

    let loaded = match Config::find_and_load_with_path() {
        Ok(loaded) => Some(loaded),
        Err(e) if e.to_string().starts_with("No maestro.toml found") => None,
        Err(e) => return Err(e),
    };

    // Workspace trust gate (#1045): before the TUI paints, show the
    // executable-bearing config values (if any) and ask for consent. The
    // decision is re-read from the trust store when the app is assembled
    // below, so a just-recorded consent takes effect immediately.
    if let Some(loaded) = &loaded {
        crate::config::trust::ensure_interactive_consent_with_default_store(
            &loaded.path,
            &loaded.config,
        );
    }

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

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root.clone()));

    let mut app = if let Some(lc) = loaded {
        let provider_config = lc.config.effective_provider_config();
        let mut app = setup_app_from_config(lc, store, worktree_mgr, None);
        app.github_client = Some(std::sync::Arc::from(create_provider(&provider_config)?));
        app
    } else {
        no_config_app(store, worktree_mgr)
    };
    // #1266: this run's state saves must not drop sessions a quit paused.
    app.pool.keep_saved_resumable(&state.sessions);

    // Wire MiniMax quota for the token dashboard's Quota column (#769).
    // Best-effort — if the file is missing or malformed the dashboard
    // falls back to the `NoQuotaSnapshots` placeholder and prints `-`.
    if let Some(quota) = crate::agent_provider::minimax::MinimaxQuota::open_default() {
        app = app.with_minimax_quota(std::sync::Arc::new(quota));
    }

    app.gh_auth_ok = gh_auth_ok;
    app.first_run = first_run;

    app.screen_state.home_screen = Some(crate::tui::screens::HomeScreen::new(
        project_info,
        recent_sessions,
        doctor_warnings,
    ));
    // The persistent Home screen (#290); `tui::run` builds it.
    app.tui_mode = crate::tui::app::TuiMode::Landing;

    app.pending_commands
        .push(crate::tui::app::TuiCommand::FetchSuggestionData);

    // #1563: last before the first screen, so its line is not lost.
    startup_cleanup(&mut app, &repo_root, false);
    crate::tui::run(app).await
}

/// No state file yet: maestro never ran (and quit) here (#1279). Every
/// quit saves the state file, so only the first run sees `true`.
fn is_first_run(state_path: &std::path::Path) -> bool {
    !state_path.exists()
}

/// App for repos without a `maestro.toml` (#1036). Seeded with the serde
/// default permission mode — bypass stays opt-in.
fn no_config_app(
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
) -> App {
    App::new(
        store,
        DEFAULT_MAX_CONCURRENT,
        worktree_mgr,
        crate::config::default_permission_mode(),
        Vec::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_is_true_only_until_state_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("maestro-state-1279.json");
        assert!(is_first_run(&path), "no state file yet");
        StateStore::new(path.clone())
            .save(&crate::state::types::MaestroState::default())
            .unwrap();
        assert!(!is_first_run(&path), "a saved state means a later run");
    }

    #[test]
    fn dashboard_fallback_without_config_does_not_bypass() {
        // #1036: with no maestro.toml, the dashboard must still start in
        // the safe permission mode; bypass is opt-in only.
        let tmp = std::env::temp_dir().join(format!(
            "maestro-1036-dashboard-fallback-{}.json",
            uuid::Uuid::new_v4()
        ));
        let app = no_config_app(
            StateStore::new(tmp),
            Box::new(crate::session::worktree::MockWorktreeManager::new()),
        );
        assert_eq!(
            app.session_config.permission_mode, "default",
            "no-config fallback must not default to bypassPermissions"
        );
    }
}

/// Saved sessions `maestro resume` would pick up, Paused included (#1266).
fn has_incomplete(sessions: &[crate::session::types::Session]) -> bool {
    use crate::session::types::SessionStatus::*;
    (sessions.iter()).any(|s| {
        matches!(
            s.status,
            Running | Paused | Spawning | Queued | Stalled | Retrying
        )
    })
}

#[cfg(test)]
mod has_incomplete_tests {
    use super::has_incomplete;
    use crate::session::transition::TransitionReason;
    use crate::session::types::{Session, SessionStatus};

    #[test]
    fn dashboard_hints_resume_for_paused() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        for to in [
            SessionStatus::Spawning,
            SessionStatus::Running,
            SessionStatus::Paused,
        ] {
            s.transition_to(to, TransitionReason::UserQuit).unwrap();
        }
        assert!(
            has_incomplete(&[s]),
            "a quit-paused session must get the resume hint"
        );
    }
}
