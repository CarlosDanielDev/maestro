use crate::commands::setup::setup_app_from_config;
use crate::config::Config;
use crate::provider::create_provider;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::state::types::MaestroState;

pub async fn cmd_resume(session_filter: Option<String>) -> anyhow::Result<()> {
    let loaded = Config::find_and_load_with_path()?;
    let provider_config = loaded.config.effective_provider_config();
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;
    let repo_root = std::env::current_dir()?;

    let incomplete = resumable_sessions(&state, session_filter.as_deref());

    if incomplete.is_empty() {
        println!("No incomplete sessions to resume.");
        return Ok(());
    }

    println!("Resuming {} incomplete session(s)...", incomplete.len());

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));
    let mut app = setup_app_from_config(loaded, store, worktree_mgr, None);

    for s in &incomplete {
        app.add_session(restore_session(s)).await?;
    }

    app.github_client = Some(std::sync::Arc::from(create_provider(&provider_config)?));

    crate::tui::run(app).await
}

/// Saved sessions `maestro resume` picks up, narrowed by an id prefix or
/// an issue number.
fn resumable_sessions<'a>(state: &'a MaestroState, filter: Option<&str>) -> Vec<&'a Session> {
    state
        .sessions
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                SessionStatus::Running
                    | SessionStatus::Paused
                    | SessionStatus::Spawning
                    | SessionStatus::Queued
                    | SessionStatus::Stalled
                    | SessionStatus::Errored
                    | SessionStatus::Retrying
            )
        })
        .filter(|s| match filter {
            Some(filter) => {
                s.id.to_string().starts_with(filter)
                    || s.issue_number.is_some_and(|n| n.to_string() == filter)
            }
            None => true,
        })
        .collect()
}

/// A fresh queued session that continues the saved one `s`. A session a
/// quit paused (#1266) keeps its id, worktree and agent conversation, so
/// it picks up where it stopped.
fn restore_session(s: &Session) -> Session {
    let mut new_session = Session::new(
        s.prompt.clone(),
        s.model.clone(),
        s.mode.clone(),
        s.issue_number,
        Some(s.role),
    );
    new_session.issue_title = s.issue_title.clone();
    new_session.retry_count = s.retry_count;
    if s.status == SessionStatus::Paused {
        new_session.id = s.id;
        new_session.worktree_path = s.worktree_path.clone();
        if let Some(id) = s.agent_session_id.as_deref() {
            new_session.bind_agent_session_id(id);
        }
    }
    new_session
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
