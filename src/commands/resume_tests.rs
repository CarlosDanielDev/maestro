//! #1266: `maestro resume` brings back a session a confirmed quit paused,
//! in its kept worktree and on its saved conversation.

use super::{restore_session, resumable_sessions};
use crate::session::pool::SessionPool;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::{MockWorktreeManager, WorktreeManager};
use crate::state::store::StateStore;

const AGENT_SESSION_ID: &str = "11111111-2222-3333-4444-555555555555";

#[test]
fn resume_restores_quit_paused_session() {
    // The worktree the paused session ran in is still on disk.
    let worktrees = MockWorktreeManager::new();
    let worktree = worktrees.create("issue-1266").unwrap();

    // What a confirmed quit leaves in state.
    let mut paused = Session::new(
        "implement #1266".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1266),
        None,
    );
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::Paused, TransitionReason::UserQuit),
    ] {
        paused.transition_to(to, why).unwrap();
    }
    assert!(paused.bind_agent_session_id(AGENT_SESSION_ID));
    paused.worktree_path = Some(worktree.clone());
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().join("state-1266.json"));
    let mut state = store.load().unwrap();
    state.sessions.push(paused.clone());
    store.save(&state).unwrap();
    let state = store.load().unwrap();

    let picked = resumable_sessions(&state, None);
    assert_eq!(picked.len(), 1, "a quit-paused session must be resumable");
    let restored = restore_session(picked[0]);
    assert_eq!(restored.status, SessionStatus::Queued);
    assert_eq!(restored.id, paused.id);
    assert_eq!(restored.agent_session_id.as_deref(), Some(AGENT_SESSION_ID));
    assert_eq!(restored.worktree_path.as_ref(), Some(&worktree));

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut pool = SessionPool::new(1, Box::new(worktrees), tx);
    pool.enqueue(restored);
    let promoted = pool.try_promote();
    let managed = pool.get_active_mut(promoted[0]).unwrap();
    assert_eq!(
        managed.worktree_path.as_ref(),
        Some(&worktree),
        "resume must run in the kept worktree"
    );
}
