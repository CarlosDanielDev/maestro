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
    let turn = managed.arm_turn_for_test();
    let worktree = managed.worktree_path.clone();
    assert!(worktree.is_some(), "setup: the mock gives a worktree");
    // A status that cannot pause: killed at quit, with the reason recorded.
    let ci_fix = session("ci-fix");
    let ci_id = ci_fix.id;
    app.pool.enqueue(ci_fix);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(ci_id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::CiFix, TransitionReason::CiFixStarted),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    let queued = session("queued");
    let queued_id = queued.id;
    app.pool.enqueue(queued);

    // Step 1 only records: nothing stops before the save.
    app.pause_for_exit();
    assert!(!turn.is_cancelled(), "nothing may stop before the save");

    save_on_exit(&mut app).expect("saved");
    assert!(turn.is_cancelled(), "the agent is stopped after the save");

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
    let ci = find(ci_id).expect("ci-fix saved");
    assert_eq!(ci.status, SessionStatus::Killed);
    assert_eq!(
        ci.transition_history.last().map(|t| t.reason),
        Some(TransitionReason::UserQuit)
    );
    assert_eq!(
        (saved.sessions.iter())
            .filter(|s| s.status == SessionStatus::Killed)
            .count(),
        1,
        "quit kills only what cannot pause"
    );
}

/// #1266: a plain `maestro` launch after a quit must not drop the Paused
/// sessions waiting for `maestro resume` when it saves state.
#[test]
fn plain_launch_keeps_saved_paused() {
    let mut paused = session("paused-earlier");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::Paused, TransitionReason::UserQuit),
    ] {
        paused.transition_to(to, why).unwrap();
    }
    let mut app = make_test_app("plain-launch-1266");
    app.pool.keep_saved_paused(std::slice::from_ref(&paused));
    app.pool.keep_saved_paused(std::slice::from_ref(&paused));

    save_on_exit(&mut app).expect("saved");

    let saved = app.store.load().expect("state saved");
    let kept: Vec<_> = (saved.sessions.iter())
        .filter(|s| s.id == paused.id)
        .collect();
    assert_eq!(kept.len(), 1, "kept once, not dropped and not doubled");
    assert_eq!(kept[0].status, SessionStatus::Paused);
}

/// #1266: the upgrade restart saves like a quit. When the exec fails the
/// stopped agents stay Paused (`r` cannot bring back a cancelled turn) and
/// the log says to quit and use `maestro resume`.
#[cfg(unix)]
#[tokio::test]
async fn restart_failure_saves_and_keeps_agents_stopped() {
    let mut app = make_test_app("restart-save-1266");
    let running = session("running");
    let id = running.id;
    app.pool.enqueue(running);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    let turn = managed.arm_turn_for_test();

    let result = save_and_restart(&mut app, || anyhow::bail!("exec failed"));

    assert!(result.is_err());
    assert!(turn.is_cancelled(), "the restart stops the agents");
    let saved = app.store.load().expect("state saved");
    let status = saved.sessions.iter().find(|s| s.id == id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Paused), "saved before the exec");
    assert!(
        (app.activity_log.entries().iter())
            .any(|e| e.message.contains("quit, then run maestro resume")),
        "the log must say how to get the agents back"
    );
    app.resume_all();
    assert_eq!(
        app.pool.get_session(id).map(|s| s.status),
        Some(SessionStatus::Paused),
        "r must not mark a cancelled turn Running"
    );
}

/// #1266: statuses that cannot move to Killed (no agent process) are left
/// alone at quit, with no "Killed at quit" line.
#[tokio::test]
async fn quit_leaves_gates_running_alone() {
    let mut app = make_test_app("quit-gates-1266");
    let gates = session("gates");
    let id = gates.id;
    app.pool.enqueue(gates);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::GatesRunning, TransitionReason::GatesStarted),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }

    save_on_exit(&mut app).expect("saved");

    let s = app.pool.get_session(id).unwrap();
    assert_eq!(s.status, SessionStatus::GatesRunning);
    assert!(
        !(s.activity_log.iter()).any(|e| e.message.contains("Killed at quit")),
        "no kill line for a session that was not killed"
    );
}

fn running_session(app: &mut App) -> uuid::Uuid {
    let running = session("running");
    let id = running.id;
    app.pool.enqueue(running);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    id
}

fn paused_on_disk(store: &crate::state::store::StateStore, id: uuid::Uuid) -> bool {
    (store.load().ok()).is_some_and(|state| {
        (state.sessions.iter()).any(|s| s.id == id && s.status == SessionStatus::Paused)
    })
}

/// #1266 judge: the Paused records are on disk at the moment the agents
/// stop (the exit's only save runs before the stop).
#[test]
fn exit_saves_before_it_stops_agents() {
    let mut app = make_test_app("exit-order-1266");
    let id = running_session(&mut app);
    let mut on_disk_at_stop = None;

    save_then(&mut app, |app| {
        on_disk_at_stop = Some(paused_on_disk(&app.store, id));
    })
    .expect("saved");

    assert_eq!(on_disk_at_stop, Some(true), "stop ran before the save");
}

/// #1266 judge: the upgrade-restart key saves (and stops) before the exec;
/// any other key does nothing.
#[test]
fn restart_key_saves_before_exec() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = make_test_app("restart-key-1266");
    let id = running_session(&mut app);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state-1266.json");
    app.store = crate::state::store::StateStore::new(path.clone());
    let probe = crate::state::store::StateStore::new(path);
    app.upgrade_state = crate::updater::UpgradeState::ReadyToRestart {
        version: "9.9.9".into(),
        backup_path: "/nonexistent-1266".into(),
    };
    let other = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(on_restart_key(&mut app, &other, || unreachable!()).is_none());

    let mut on_disk_at_exec = false;
    let yes = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    let result = on_restart_key(&mut app, &yes, || {
        on_disk_at_exec = paused_on_disk(&probe, id);
        Ok(())
    });

    assert!(matches!(result, Some(Ok(()))));
    assert!(on_disk_at_exec, "the exec ran before the state was saved");
}

/// #1266 judge: when the exit save fails, `save_on_exit` errs with a clear
/// message (the run exits non-zero with it) and never stays silent.
#[test]
fn failed_exit_save_is_an_error() {
    let mut app = make_test_app("exit-save-fails-1266");
    let _ = running_session(&mut app);
    let blocker = tempfile::NamedTempFile::new().unwrap();
    // A path under a regular file cannot be written.
    app.store = crate::state::store::StateStore::new(blocker.path().join("state.json"));

    let err = save_on_exit(&mut app).expect_err("a failed save must not be silent");

    let msg = err.to_string();
    assert!(msg.contains("could not save maestro state"), "{msg}");
    assert!(msg.contains("1 paused agent(s) are not recorded"), "{msg}");
}
