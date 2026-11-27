//! #1487: a `maestro resume` run keeps every saved record it could pick
//! up and does not restore, whatever its status — so a later resume still
//! finds it. Real git repo, real state file, `cmd_resume`'s own steps.

use super::tests::{init_repo, issue_session, walk};
use super::{apply_plan, plan_resume, sessions_to_resume};
use crate::commands::setup::setup_app_from_config;
use crate::config::{Config, LoadedConfig};
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use std::path::Path;

const AGENT_SESSION_ID: &str = "11111111-2222-3333-4444-000000001487";

/// A session another resume brings back: paused at quit, bound to its
/// agent conversation.
fn bound_paused(n: u64) -> Session {
    use SessionStatus::{Paused, Running, Spawning};
    let mut s = issue_session(n);
    walk(&mut s, &[Spawning, Running, Paused]);
    s.started_at = Some(chrono::Utc::now());
    assert!(s.bind_agent_session_id(AGENT_SESSION_ID));
    s
}

/// Saves `sessions` as an earlier run left them, then runs `maestro
/// resume [filter]` through `cmd_resume`'s own `apply_plan` and saves
/// once. Returns the state after that save.
async fn resume_run(sessions: Vec<Session>, filter: Option<&str>) -> MaestroState {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1487.json");
    let store = StateStore::new(store_path.clone());
    let mut state = store.load().unwrap();
    state.sessions = sessions;
    store.save(&state).unwrap();

    let state = store.load().unwrap();
    let (picked, _) = sessions_to_resume(&state, filter, false);
    assert_eq!(picked.len(), 1, "setup: one session resumes");
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    let plan = plan_resume(&state, &picked, &probe);
    assert!(plan.refused.is_empty(), "setup: {:?}", plan.refused);
    let mut app = app_at!(repo.path(), &store_path);
    app.workspace_trusted = false; // nothing may spawn here
    for s in apply_plan(&mut app.pool, plan) {
        app.add_session(s).await.unwrap();
    }
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);
    StateStore::new(store_path).load().unwrap()
}

fn saved(state: &MaestroState, id: uuid::Uuid) -> Option<&Session> {
    state.sessions.iter().find(|s| s.id == id)
}

/// A crash mid-first-turn leaves a Running record with no agent session
/// id and no recorded worktree. Resume skips it (no `--rerun-first-turn`)
/// while another session resumes; the record must still be saved.
#[tokio::test]
async fn crash_running_first_turn_survives_resume_run() {
    use SessionStatus::{Running, Spawning};
    let mut crashed = issue_session(1487);
    walk(&mut crashed, &[Spawning, Running]);
    crashed.started_at = Some(chrono::Utc::now()); // spawned, no id yet
    let id = crashed.id;

    let state = resume_run(vec![crashed, bound_paused(1488)], None).await;

    let kept = saved(&state, id).expect("the crashed record is still saved");
    assert_eq!(kept.status, Running, "as the crash left it");
    let (picked, _) = sessions_to_resume(&state, None, true);
    assert!(
        picked.iter().any(|s| s.id == id),
        "--rerun-first-turn finds it"
    );
}

/// An Errored record not stopped by the budget cap, left out by an issue
/// filter while another session resumes: it must still be saved. A
/// Completed record is not something resume picks up, so it is not kept.
#[tokio::test]
async fn errored_not_at_cap_survives_resume_run() {
    use SessionStatus::{Completed, Errored, Running, Spawning};
    let mut errored = issue_session(1489);
    walk(&mut errored, &[Spawning, Running, Errored]);
    errored.started_at = Some(chrono::Utc::now());
    assert!(errored.bind_agent_session_id(AGENT_SESSION_ID));
    let mut done = issue_session(1490);
    walk(&mut done, &[Spawning, Running, Completed]);
    let (id, done_id) = (errored.id, done.id);

    let state = resume_run(vec![errored, done, bound_paused(1491)], Some("1491")).await;

    let kept = saved(&state, id).expect("the Errored record is still saved");
    assert_eq!(kept.status, Errored, "as it was saved");
    let (picked, _) = sessions_to_resume(&state, Some("1489"), false);
    assert_eq!(picked.len(), 1, "a later `maestro resume 1489` finds it");
    assert!(saved(&state, done_id).is_none(), "a finished run is not kept");
}
