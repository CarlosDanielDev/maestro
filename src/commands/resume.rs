use crate::commands::setup::setup_app_from_config;
use crate::config::Config;
use crate::provider::github::client::GhCliClient;
use crate::session::types::Session;
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;

pub async fn cmd_resume(session_filter: Option<String>) -> anyhow::Result<()> {
    let loaded = Config::find_and_load_with_path()?;
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;
    let repo_root = std::env::current_dir()?;

    let incomplete: Vec<&Session> = state
        .sessions
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                crate::session::types::SessionStatus::Running
                    | crate::session::types::SessionStatus::Spawning
                    | crate::session::types::SessionStatus::Queued
                    | crate::session::types::SessionStatus::Stalled
                    | crate::session::types::SessionStatus::Errored
                    | crate::session::types::SessionStatus::Retrying
            )
        })
        .filter(|s| {
            if let Some(ref filter) = session_filter {
                s.id.to_string().starts_with(filter)
                    || s.issue_number
                        .map(|n| n.to_string() == *filter)
                        .unwrap_or(false)
            } else {
                true
            }
        })
        .collect();

    if incomplete.is_empty() {
        println!("No incomplete sessions to resume.");
        return Ok(());
    }

    println!("Resuming {} incomplete session(s)...", incomplete.len());

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));
    let mut app = setup_app_from_config(loaded, store, worktree_mgr, None);

    for s in &incomplete {
        let mut new_session = Session::new(
            s.prompt.clone(),
            s.model.clone(),
            s.mode.clone(),
            s.issue_number,
        );
        new_session.issue_title = s.issue_title.clone();
        new_session.retry_count = s.retry_count;
        app.add_session(new_session).await?;
    }

    app.github_client = Some(Box::new(GhCliClient::new()));

    crate::tui::run(app).await
}
