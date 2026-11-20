use crate::commands::setup::setup_app_from_config;
use crate::config::Config;
use crate::provider::create_provider;
use crate::session::pool::session_slug;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::{GitWorktreeManager, WorktreeManager};
use crate::state::store::StateStore;
use crate::state::types::MaestroState;

pub async fn cmd_resume(
    session_filter: Option<String>,
    rerun_first_turn: bool,
) -> anyhow::Result<()> {
    let loaded = Config::find_and_load_with_path()?;
    let provider_config = loaded.config.effective_provider_config();
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;
    let repo_root = std::env::current_dir()?;

    let (incomplete, notes) = select_first_turn(
        resumable_sessions(&state, session_filter.as_deref()),
        rerun_first_turn,
    );
    for note in &notes {
        println!("{note}");
    }

    if incomplete.is_empty() {
        println!("No incomplete sessions to resume.");
        return Ok(());
    }

    println!("Resuming {} incomplete session(s)...", incomplete.len());

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root.clone()));
    let mut app = setup_app_from_config(loaded, store, worktree_mgr, None);

    let probe = GitWorktreeManager::new(repo_root);
    for s in &incomplete {
        match restore_session(s, &probe) {
            Ok(session) => app.add_session(session).await?,
            Err(why) => eprintln!("{why}"),
        }
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

/// Splits off sessions saved during their first turn (#1424): spawned
/// but with no agent session id, resuming one runs its first prompt again
/// and spends again. They are skipped unless `rerun_first_turn` is set;
/// either way each gets a line saying so.
fn select_first_turn(
    sessions: Vec<&Session>,
    rerun_first_turn: bool,
) -> (Vec<&Session>, Vec<String>) {
    let mut notes = Vec::new();
    let picked = sessions
        .into_iter()
        .filter(|s| {
            // Bound, or never spawned: a never-run prompt spends only once.
            if s.agent_session_id.is_some() || s.started_at.is_none() {
                return true;
            }
            let label = match s.issue_number {
                Some(n) => format!("#{n}"),
                None => session_slug(s),
            };
            let tail = if rerun_first_turn {
                "re-running it (--rerun-first-turn)"
            } else {
                "skipped; pass --rerun-first-turn to include it"
            };
            notes.push(format!(
                "{label} has no saved agent session — resuming re-runs its first \
                 prompt (spends again); {tail}."
            ));
            rerun_first_turn
        })
        .collect();
    (picked, notes)
}

/// A fresh queued session that continues the saved one `s` (#1266): it
/// keeps its id, worktree and agent conversation, so it picks up in its
/// own kept worktree. Refused when that worktree is still on disk but was
/// never recorded: the pool could not reuse it and would run the agent in
/// the main checkout.
fn restore_session(s: &Session, worktrees: &dyn WorktreeManager) -> Result<Session, String> {
    let slug = session_slug(s);
    if s.worktree_path.is_none() && worktrees.exists(&slug) {
        return Err(format!(
            "Skipping {slug}: its worktree is still on disk but not recorded in state; \
             work in it by hand, or remove that worktree and resume again."
        ));
    }
    let mut new_session = Session::new(
        s.prompt.clone(),
        s.model.clone(),
        s.mode.clone(),
        s.issue_number,
        Some(s.role),
    );
    new_session.issue_title = s.issue_title.clone();
    new_session.retry_count = s.retry_count;
    new_session.id = s.id;
    new_session.worktree_path = s.worktree_path.clone();
    if let Some(id) = s.agent_session_id.as_deref() {
        new_session.bind_agent_session_id(id);
    }
    Ok(new_session)
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
