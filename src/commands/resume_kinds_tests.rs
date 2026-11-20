//! #1423: `maestro resume` brings back Interactive, CiFix and ConflictFix
//! sessions a quit saved — each in its own kept worktree (real git), an
//! idle chat without sending a turn — and never runs one in the main
//! checkout.

use super::tests::{init_repo, issue_session, walk};
use super::{restore_session, resumable_sessions};
use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::commands::setup::setup_app_from_config;
use crate::config::{Config, LoadedConfig};
use crate::session::interaction::{TurnRecord, TurnRole};
use crate::session::types::{
    CiFixContext, ConflictFixContext, Session, SessionMode, SessionStatus,
};
use crate::session::worktree::{GitWorktreeManager, WorktreeManager};
use crate::state::store::StateStore;
use crate::tui::app::App;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CHAT_ID: &str = "aaaaaaaa-0000-0000-0000-000000001423";
const CI_ID: &str = "bbbbbbbb-0000-0000-0000-000000001423";
const CONFLICT_ID: &str = "cccccccc-0000-0000-0000-000000001423";

/// An app on `repo` whose agent command does not exist: nothing here may
/// start a real agent.
fn app_at(repo: &Path, store_path: &Path) -> App {
    let config: Config = toml::from_str(
        "[project]\nrepo = \"owner/repo\"\n[sessions]\nmax_concurrent = 4\n[budget]\n[notifications]\n\
         [agents]\ndefault = \"q\"\n[agents.q]\nkind = \"qwen\"\n\
         command = \"maestro-missing-agent-1423\"\n",
    )
    .unwrap();
    let loaded = LoadedConfig {
        config,
        path: store_path.with_file_name("maestro.toml"),
    };
    let worktrees = Box::new(GitWorktreeManager::new(repo.to_path_buf()));
    setup_app_from_config(loaded, StateStore::new(store_path.into()), worktrees, None)
}

/// An idle chat (settled into Interactive, two saved turns), a CI fix and a
/// conflict fix, each bound to an agent session id. Returns their ids and
/// kept worktrees, in that order.
fn chat_and_fixes(app: &mut App) -> ([uuid::Uuid; 3], Vec<PathBuf>) {
    use SessionStatus::{CiFix, Completed, ConflictFix, Running, Spawning};
    let chat = (app.pool).create_interaction_session(
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
        app.pool.enqueue(s);
    }
    let _ = app.pool.try_promote();
    let walks: [&[SessionStatus]; 3] = [&[Spawning, Running, Completed], &[CiFix], &[ConflictFix]];
    let mut kept = Vec::new();
    for ((id, steps), agent_id) in ids.iter().zip(walks).zip([CHAT_ID, CI_ID, CONFLICT_ID]) {
        let managed = app.pool.get_active_mut(*id).unwrap();
        walk(&mut managed.session, steps);
        assert!(managed.session.bind_agent_session_id(agent_id));
        kept.push(managed.worktree_path.clone().unwrap());
    }
    let chat = &mut app.pool.get_active_mut(ids[0]).unwrap().session;
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

/// Every session `maestro resume` would restart from `store_path`.
fn restored(repo: &Path, store_path: &Path) -> Vec<Session> {
    let state = StateStore::new(store_path.into()).load().unwrap();
    let probe = GitWorktreeManager::new(repo.to_path_buf());
    (resumable_sessions(&state, None).iter())
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
    let mut app = app_at(repo.path(), &store_path);
    let (ids, kept) = chat_and_fixes(&mut app);
    std::fs::write(kept[0].join("chat.txt"), "uncommitted").unwrap();
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    let sessions = restored(repo.path(), &store_path);
    assert_eq!(sessions.len(), 3, "all three kinds resume");
    let mut app = app_at(repo.path(), &store_path);
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
    let resumed: Vec<_> = (agent.requests.lock().unwrap().iter())
        .map(|r| r.resume_session_id.clone())
        .collect();
    assert_eq!(resumed.len(), 2, "only the two fixes start: {resumed:?}");
    for id in [CI_ID, CONFLICT_ID] {
        assert!(resumed.contains(&Some(id.to_string())), "{id}: {resumed:?}");
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
    assert_eq!(chat.session.agent_session_id.as_deref(), Some(CHAT_ID));
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
    let mut app = app_at(repo.path(), &store_path);
    let (ids, _) = chat_and_fixes(&mut app);
    let stalled = issue_session(1426);
    let stalled_id = stalled.id;
    app.pool.enqueue(stalled);
    let _ = app.pool.try_promote();
    walk(
        &mut app.pool.get_active_mut(stalled_id).unwrap().session,
        &[Spawning, Running, Stalled],
    );
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    // A plain launch, as `maestro` / `maestro run` set it up.
    let mut app = app_at(repo.path(), &store_path);
    let saved = app.state.sessions.clone();
    app.pool.keep_saved_resumable(&saved);
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    let back: Vec<_> = (restored(repo.path(), &store_path).iter())
        .map(|s| s.id)
        .collect();
    for id in ids.iter().chain([&stalled_id]) {
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
    let mut app = app_at(repo.path(), &store_path);
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
