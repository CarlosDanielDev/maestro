//! #1423: `maestro resume` brings back Interactive, CiFix and ConflictFix
//! sessions a quit saved — each in its own kept worktree (real git), an
//! idle chat without sending a turn — and never runs one in the main
//! checkout.

use super::tests::{init_repo, issue_session, walk};
use super::{plan_resume, restore_session, resumable_sessions, sessions_to_resume};
use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::commands::setup::setup_app_from_config;
use crate::config::{Config, LoadedConfig};
use crate::session::interaction::{TurnRecord, TurnRole};
use crate::session::pool::SessionPool;
use crate::session::types::{
    CiFixContext, ConflictFixContext, Session, SessionMode, SessionStatus,
};
use crate::session::worktree::{GitWorktreeManager, WorktreeManager};
use crate::state::store::StateStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CHAT_ID: &str = "aaaaaaaa-0000-0000-0000-000000001423";

/// An app on `repo` whose agent command does not exist: nothing here may
/// start a real agent. A macro, so this test (layer 3) never names the TUI
/// `App` type.
macro_rules! app_at {
    ($repo:expr, $store_path:expr) => {{
        let config: Config = toml::from_str(
            "[project]\nrepo = \"owner/repo\"\n[sessions]\nmax_concurrent = 5\n[budget]\n\
             [notifications]\n[agents]\ndefault = \"q\"\n[agents.q]\nkind = \"qwen\"\n\
             command = \"maestro-missing-agent-1423\"\n",
        )
        .unwrap();
        let store_path: &Path = $store_path;
        let loaded = LoadedConfig {
            config,
            path: store_path.with_file_name("maestro.toml"),
        };
        let worktrees = Box::new(GitWorktreeManager::new(Path::to_path_buf($repo)));
        setup_app_from_config(loaded, StateStore::new(store_path.into()), worktrees, None)
    }};
}

/// An idle chat (spawned, settled into Interactive, two saved turns), a CI
/// fix and a conflict fix (never spawned), on the qwen agent, which never
/// returns an agent session id. Returns their ids and kept worktrees, in
/// that order.
fn chat_and_fixes(pool: &mut SessionPool) -> ([uuid::Uuid; 3], Vec<PathBuf>) {
    use SessionStatus::{CiFix, Completed, ConflictFix, Running, Spawning};
    let chat = pool.create_interaction_session(
        1423,
        false,
        "opus".into(),
        "orchestrator".into(),
        Some("q".into()),
    );
    let mut ci = issue_session(1424);
    ci.ci_fix_context = Some(CiFixContext {
        pr_number: 9,
        issue_number: 1424,
        branch: "maestro/issue-1424".into(),
        attempt: 1,
    });
    let mut conflict = issue_session(1425);
    conflict.conflict_fix_context = Some(ConflictFixContext {
        pr_number: 10,
        issue_number: 1425,
        branch: "maestro/issue-1425".into(),
        conflicting_files: vec!["README.md".into()],
    });
    let ids = [chat, ci.id, conflict.id];
    for s in [ci, conflict] {
        pool.enqueue(s);
    }
    let _ = pool.try_promote();
    let walks: [&[SessionStatus]; 3] = [&[Spawning, Running, Completed], &[CiFix], &[ConflictFix]];
    let mut kept = Vec::new();
    for (id, steps) in ids.iter().zip(walks) {
        let managed = pool.get_active_mut(*id).unwrap();
        walk(&mut managed.session, steps);
        kept.push(managed.worktree_path.clone().unwrap());
    }
    let chat = &mut pool.get_active_mut(ids[0]).unwrap().session;
    chat.started_at = Some(chrono::Utc::now()); // its first turn ran
    assert_eq!(chat.status, SessionStatus::Interactive, "setup: idle chat");
    for (role, content) in [(TurnRole::User, "fix it"), (TurnRole::Agent, "done")] {
        chat.turns.push(TurnRecord {
            role,
            content: content.into(),
            started_at: chrono::Utc::now(),
            finished_at: Some(chrono::Utc::now()),
        });
    }
    (ids, kept)
}

/// Every session `maestro resume` would restart from `store_path`, as
/// `cmd_resume` picks them (no `--rerun-first-turn`).
fn restored(repo: &Path, store_path: &Path) -> Vec<Session> {
    let state = StateStore::new(store_path.into()).load().unwrap();
    let probe = GitWorktreeManager::new(repo.to_path_buf());
    let (picked, notes) = sessions_to_resume(&state, None, false);
    assert!(notes.is_empty(), "nothing here is a first turn: {notes:?}");
    (picked.iter())
        .map(|s| restore_session(s, &probe).expect("restorable"))
        .collect()
}

/// The quit, then `maestro resume` through the real path (`add_session`,
/// as `cmd_resume` does). The fixes start again in their worktrees; the
/// idle chat comes back waiting for the user: no turn is sent.
#[tokio::test]
async fn resume_restores_interactive_and_fix_sessions() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1423.json");
    let mut app = app_at!(repo.path(), &store_path);
    let (ids, kept) = chat_and_fixes(&mut app.pool);
    std::fs::write(kept[0].join("chat.txt"), "uncommitted").unwrap();
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    let sessions = restored(repo.path(), &store_path);
    assert_eq!(sessions.len(), 3, "all three kinds resume");
    let mut app = app_at!(repo.path(), &store_path);
    app.workspace_trusted = true;
    let agent = Arc::new(ScriptedProvider::new(
        (0..3)
            .map(|_| ScriptedTurn {
                events: vec![],
                end: ScriptedEnd::WaitForCancel,
            })
            .collect(),
    ));
    app.pool.set_provider(agent.clone());
    (app.pool).set_agent_providers([("q".to_string(), agent.clone() as _)].into());
    for s in sessions {
        app.add_session(s).await.unwrap();
    }
    let sent = || agent.requests.lock().unwrap().len();
    for _ in 0..200 {
        if sent() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let cwds: Vec<_> = (agent.requests.lock().unwrap().iter())
        .map(|r| r.cwd.clone())
        .collect();
    assert_eq!(cwds.len(), 2, "only the two fixes start: {cwds:?}");
    for path in &kept[1..] {
        assert!(cwds.contains(&Some(path.clone())), "{path:?}: {cwds:?}");
    }

    for (id, path) in ids.iter().zip(&kept) {
        let managed = app.pool.get_active_mut(*id).expect("restored with its id");
        assert_eq!(
            managed.worktree_path.as_ref(),
            Some(path),
            "a resumed session must run in its own kept worktree, never the main checkout"
        );
    }
    let chat = app
        .pool
        .interactive_managed(1423)
        .expect("the chat is back");
    assert_eq!(chat.session.id, ids[0]);
    assert!(!chat.has_live_turn(), "an idle chat sends no turn");
    assert_eq!(chat.session.status, SessionStatus::Interactive);
    assert_eq!(chat.session.session_mode, SessionMode::Interactive);
    assert_eq!(chat.session.turns.len(), 2, "its saved turns");
    assert_eq!(chat.session.agent_id.as_deref(), Some("q"), "same agent");
    let ci = &app.pool.get_active_mut(ids[1]).unwrap().session;
    assert!(ci.ci_fix_context.is_some(), "still a CI fix");
    let conflict = &app.pool.get_active_mut(ids[2]).unwrap().session;
    assert!(
        conflict.conflict_fix_context.is_some(),
        "still a conflict fix"
    );
    assert!(kept[0].join("chat.txt").exists(), "the kept work survives");
    app.pool.stop_all_turns();
}

/// A plain `maestro` launch between the quit and `maestro resume` keeps
/// every record resume would pick up.
#[tokio::test]
async fn chat_and_fix_sessions_survive_a_plain_launch() {
    use SessionStatus::{Running, Spawning, Stalled};
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1423.json");
    let mut app = app_at!(repo.path(), &store_path);
    let (ids, _) = chat_and_fixes(&mut app.pool);
    let (stalled, spawning) = (issue_session(1426), issue_session(1434));
    let (stalled_id, spawning_id) = (stalled.id, spawning.id);
    app.pool.enqueue(stalled);
    app.pool.enqueue(spawning);
    let _ = app.pool.try_promote();
    walk(
        &mut app.pool.get_active_mut(stalled_id).unwrap().session,
        &[Spawning, Running, Stalled],
    );
    walk(
        &mut app.pool.get_active_mut(spawning_id).unwrap().session,
        &[Spawning],
    );
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    // A plain launch, as `maestro` / `maestro run` set it up.
    let mut app = app_at!(repo.path(), &store_path);
    let saved = app.state.sessions.clone();
    app.pool.keep_saved_resumable(&saved);
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    let back: Vec<_> = (restored(repo.path(), &store_path).iter())
        .map(|s| s.id)
        .collect();
    for id in ids.iter().chain([&stalled_id, &spawning_id]) {
        assert!(back.contains(id), "{id} was dropped by the plain launch");
    }
}

/// A chat whose worktree could not be created runs in the main checkout
/// (`.`). A quit never records that as its worktree, and resume refuses
/// it instead of running the agent there.
#[test]
fn resume_never_runs_a_chat_in_the_main_checkout() {
    use SessionStatus::{Completed, Running, Spawning};
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    probe.create("issue-1427").unwrap(); // so the chat falls back to `.`
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1423.json");
    let mut app = app_at!(repo.path(), &store_path);
    let id = (app.pool).create_interaction_session(
        1427,
        false,
        "opus".into(),
        "orchestrator".into(),
        None,
    );
    let managed = app.pool.get_active_mut(id).unwrap();
    assert_eq!(managed.worktree_path, Some(PathBuf::from(".")), "setup");
    walk(&mut managed.session, &[Spawning, Running, Completed]);
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    let state = StateStore::new(store_path).load().unwrap();
    let saved = state.sessions.iter().find(|s| s.id == id).unwrap();
    assert_eq!(saved.worktree_path, None, "`.` is never recorded");
    let refused = restore_session(saved, &probe).expect_err("must not run in `.`");
    assert!(refused.contains("issue-1427"), "{refused}");
    // Nothing on disk to reuse: still refused, not restarted in a fresh
    // worktree where its conversation is not.
    let other = tempfile::tempdir().unwrap();
    init_repo(other.path());
    let fresh = GitWorktreeManager::new(other.path().to_path_buf());
    let refused = restore_session(saved, &fresh).expect_err("ran in `.`");
    assert!(refused.contains("main checkout"), "{refused}");

    for bad in [PathBuf::from("."), repo.path().to_path_buf()] {
        let mut s = saved.clone();
        s.worktree_path = Some(bad.clone());
        let refused = restore_session(&s, &probe).expect_err("not its worktree");
        assert!(refused.contains("not its worktree"), "{bad:?}: {refused}");
    }
}

/// An adopted conversation is never picked up by `maestro resume`, even
/// when a crash left it saved live: it runs in its transcript's cwd, and
/// resume would run it in a worktree instead.
#[test]
fn resume_skips_an_adopted_conversation() {
    let mut state = crate::state::types::MaestroState::default();
    let mut adopted = issue_session(1426);
    adopted.session_mode = SessionMode::Interactive;
    adopted.adopted_cwd = Some(std::env::temp_dir());
    walk(
        &mut adopted,
        &[SessionStatus::Spawning, SessionStatus::Running],
    );
    state.sessions.push(adopted);
    assert!(resumable_sessions(&state, None).is_empty());
}

/// An idle chat resumes whether or not its agent returned a session id
/// (codex, minimax, ollama, opencode and qwen never do; a Claude chat
/// whose first turn errored has none): it comes back waiting, so nothing
/// re-runs and there is no first-turn note. A saved id is bound again.
#[test]
fn idle_chat_without_agent_id_resumes() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    let mut state = crate::state::types::MaestroState::default();
    for (n, id) in [(1430, None), (1431, Some(CHAT_ID))] {
        let mut chat = issue_session(n);
        chat.session_mode = SessionMode::Interactive;
        walk(
            &mut chat,
            &[
                SessionStatus::Spawning,
                SessionStatus::Running,
                SessionStatus::Completed,
            ],
        );
        chat.started_at = Some(chrono::Utc::now());
        chat.worktree_path = Some(probe.create(&format!("issue-{n}")).unwrap());
        if let Some(id) = id {
            assert!(chat.bind_agent_session_id(id));
        }
        state.sessions.push(chat);
    }
    let (picked, notes) = sessions_to_resume(&state, None, false);
    assert!(
        notes.is_empty(),
        "no first-turn note for an idle chat: {notes:?}"
    );
    assert_eq!(picked.len(), 2, "both chats resume");
    let back: Vec<_> = (picked.iter())
        .map(|s| restore_session(s, &probe).expect("restorable"))
        .collect();
    assert!(back.iter().all(|s| s.status == SessionStatus::Interactive));
    assert_eq!(back[0].agent_session_id, None);
    assert_eq!(back[1].agent_session_id.as_deref(), Some(CHAT_ID));

    // Its worktree is gone: refused now, not a spawn error on the next
    // message.
    probe.remove("issue-1430").unwrap();
    assert!(!picked[0].worktree_path.as_ref().unwrap().exists(), "setup");
    let refused = restore_session(picked[0], &probe).expect_err("its worktree is gone");
    assert!(refused.contains("worktree is gone"), "{refused}");
}

/// `maestro resume` keeps the records it does not restore (#1423): a
/// first-turn session it skips is still in state after the first save,
/// so `--rerun-first-turn` finds it later.
#[tokio::test]
async fn resume_keeps_the_records_it_skips() {
    use SessionStatus::{CiFix, Running, Spawning};
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1423.json");
    let mut app = app_at!(repo.path(), &store_path);
    let (first, fix) = (issue_session(1432), issue_session(1433));
    let ids = [first.id, fix.id];
    for s in [first, fix] {
        app.pool.enqueue(s);
    }
    let _ = app.pool.try_promote();
    let first = &mut app.pool.get_active_mut(ids[0]).unwrap().session;
    walk(first, &[Spawning, Running]);
    first.started_at = Some(chrono::Utc::now()); // quit mid-first-turn
    walk(
        &mut app.pool.get_active_mut(ids[1]).unwrap().session,
        &[CiFix],
    );
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    // `maestro resume`, as `cmd_resume` runs it.
    let state = StateStore::new(store_path.clone()).load().unwrap();
    let (picked, notes) = sessions_to_resume(&state, None, false);
    assert_eq!(notes.len(), 1, "setup: the first turn is skipped");
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    let plan = plan_resume(&state, &picked, &probe);
    let mut app = app_at!(repo.path(), &store_path);
    app.workspace_trusted = false; // nothing may spawn here
    app.pool.keep_saved_resumable(&plan.keep);
    for s in plan.restore {
        app.add_session(s).await.unwrap();
    }
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    let state = StateStore::new(store_path).load().unwrap();
    let skipped = (state.sessions.iter())
        .find(|s| s.id == ids[0])
        .expect("the skipped record is still saved");
    assert!(skipped.worktree_path.is_some(), "with its worktree");
    let (picked, _) = sessions_to_resume(&state, None, true);
    assert!(
        picked.iter().any(|s| s.id == ids[0]),
        "--rerun-first-turn finds it"
    );
}
