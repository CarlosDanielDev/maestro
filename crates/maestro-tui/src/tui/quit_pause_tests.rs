//! #1266: a confirmed quit pauses running agents and saves them for
//! `maestro resume`; it never kills them.

use super::*;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};

fn session(prompt: &str) -> Session {
    Session::new(
        prompt.into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    )
}

#[tokio::test]
async fn quit_pauses_not_kills() {
    let mut app = make_test_app("quit-pause-1266");
    let running = session("running");
    let running_id = running.id;
    app.pool.enqueue(running);
    // Promoted but never spawned: parked at the pre-spawn budget gate.
    let parked = session("parked");
    let parked_id = parked.id;
    app.pool.enqueue(parked);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(running_id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    let worktree = managed.worktree_path.clone();
    assert!(worktree.is_some(), "setup: the mock gives a worktree");
    let queued = session("queued");
    let queued_id = queued.id;
    app.pool.enqueue(queued);

    save_on_exit(&mut app).await;

    let saved = app.store.load().expect("state saved");
    let find = |id| saved.sessions.iter().find(|s| s.id == id);
    let paused = find(running_id).expect("running session saved");
    assert_eq!(paused.status, SessionStatus::Paused, "quit must pause");
    assert_eq!(
        paused.transition_history.last().map(|t| t.reason),
        Some(TransitionReason::UserQuit)
    );
    assert_eq!(paused.worktree_path, worktree, "resume needs the worktree");
    assert_eq!(
        find(queued_id).map(|s| s.status),
        Some(SessionStatus::Queued),
        "queued work stays queued for resume"
    );
    assert_eq!(
        find(parked_id).map(|s| s.status),
        Some(SessionStatus::Queued),
        "a session parked at the budget gate stays queued"
    );
    assert!(
        !saved
            .sessions
            .iter()
            .any(|s| s.status == SessionStatus::Killed),
        "quit must not kill"
    );
}
