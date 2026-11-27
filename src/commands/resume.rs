use crate::commands::setup::setup_app_from_config;
use crate::config::Config;
use crate::provider::create_provider;
use crate::session::pool::{SessionPool, session_slug};
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

    let (incomplete, notes) =
        sessions_to_resume(&state, session_filter.as_deref(), rerun_first_turn);
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
    let plan = plan_resume(&state, &incomplete, &probe);
    for session in apply_plan(&mut app.pool, plan) {
        app.add_session(session).await?;
    }

    app.github_client = Some(std::sync::Arc::from(create_provider(&provider_config)?));

    crate::tui::run(app).await
}

/// What `cmd_resume` restores, plus one line per first-turn session it
/// skips or re-runs (#1424).
fn sessions_to_resume<'a>(
    state: &'a MaestroState,
    filter: Option<&str>,
    rerun_first_turn: bool,
) -> (Vec<&'a Session>, Vec<String>) {
    select_first_turn(resumable_sessions(state, filter), rerun_first_turn)
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
                    | SessionStatus::CiFix
                    | SessionStatus::ConflictFix
                    | SessionStatus::Interactive
            )
        })
        // An adopted conversation lives in its transcript's cwd, never a
        // worktree (#1423): resume would run it in the wrong place.
        .filter(|s| s.adopted_cwd.is_none())
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
            // An idle chat comes back waiting for the user (#1423): no turn
            // runs, with or without an id (most agents return none).
            if s.agent_session_id.is_some()
                || s.started_at.is_none()
                || s.status == SessionStatus::Interactive
            {
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

/// What `cmd_resume` does with the sessions it picked (#1423): each one
/// restored, or a line saying why not, plus every saved record a later
/// resume could pick up and this one does not restore, for
/// `SessionPool::keep_saved` (#1487) — so this run's state saves keep a
/// skipped, filtered-out or refused record, with or without a worktree.
struct ResumePlan {
    restore: Vec<Session>,
    refused: Vec<String>,
    keep: Vec<Session>,
}

fn plan_resume(
    state: &MaestroState,
    picked: &[&Session],
    worktrees: &dyn WorktreeManager,
) -> ResumePlan {
    let (mut restore, mut refused) = (Vec::new(), Vec::new());
    for s in picked {
        match restore_session(s, worktrees) {
            Ok(session) => restore.push(session),
            Err(why) => refused.push(why),
        }
    }
    let keep = (resumable_sessions(state, None).into_iter())
        .filter(|s| !restore.iter().any(|r| r.id == s.id))
        .cloned()
        .collect();
    ResumePlan {
        restore,
        refused,
        keep,
    }
}

/// Prints why each refused session is skipped and keeps every record the
/// plan does not restore in `pool` (#1487). Returns the sessions to
/// restore.
fn apply_plan(pool: &mut SessionPool, plan: ResumePlan) -> Vec<Session> {
    for why in &plan.refused {
        eprintln!("{why}");
    }
    pool.keep_saved(&plan.keep);
    plan.restore
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
    // A recorded path must be this session's own worktree (#1423): never
    // `.` or the repo root, so resume never runs an agent in the main
    // checkout.
    if let Some(path) = &s.worktree_path
        && !is_own_worktree(path, &slug)
    {
        return Err(format!(
            "Skipping {slug}: its recorded path {} is not its worktree \
             (.maestro/worktrees/{slug}); work in it by hand.",
            path.display()
        ));
    }
    let idle_chat = s.status == SessionStatus::Interactive;
    if idle_chat && s.worktree_path.is_none() {
        return Err(format!(
            "Skipping {slug}: this chat ran in the main checkout, not a worktree; \
             open the issue's chat again instead."
        ));
    }
    // An idle chat is never re-promoted, so nothing would make a new
    // worktree for it: its next message would fail to spawn.
    if idle_chat && !s.worktree_path.as_ref().is_some_and(|p| p.exists()) {
        return Err(format!(
            "Skipping {slug}: this chat's worktree is gone; open the issue's chat again instead."
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
    // Still the same kind of session (#1423): a chat, a CI fix or a
    // conflict fix, on the same agent, so the saved conversation resumes.
    new_session.agent_id = s.agent_id.clone();
    new_session.session_mode = s.session_mode;
    new_session.produce_pr = s.produce_pr;
    new_session.turns = s.turns.clone();
    new_session.ci_fix_context = s.ci_fix_context.clone();
    new_session.conflict_fix_context = s.conflict_fix_context.clone();
    // Keeps the first-turn marker (#1424): a re-run quit before it spawns
    // is still noted on the next resume. Spawn sets it again.
    new_session.started_at = s.started_at;
    if let Some(id) = s.agent_session_id.as_deref() {
        new_session.bind_agent_session_id(id);
    }
    if idle_chat {
        // Waiting for the user, as it was at quit: `App::add_session`
        // registers it without sending a turn.
        new_session.status = SessionStatus::Interactive;
        new_session.settled_from = s.settled_from;
        new_session.log_activity("Resumed: waiting for your next message".into());
    }
    Ok(new_session)
}

/// `path` is `<repo>/.maestro/worktrees/<slug>` and, when it is on disk, a
/// git worktree (a `.git` file, not the main checkout's `.git` directory).
fn is_own_worktree(path: &std::path::Path, slug: &str) -> bool {
    path.is_absolute()
        && path.ends_with(std::path::Path::new(".maestro/worktrees").join(slug))
        && (!path.exists() || path.join(".git").is_file())
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;

// `macro_use`: `keep_tests` builds its app with `kinds_tests`' `app_at!`.
#[cfg(test)]
#[macro_use]
#[path = "resume_kinds_tests.rs"]
mod kinds_tests;

#[cfg(test)]
#[path = "resume_keep_tests.rs"]
mod keep_tests;
