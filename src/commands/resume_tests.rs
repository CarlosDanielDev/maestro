//! #1266: `maestro resume` brings back what a confirmed quit saved, each
//! session in its own kept worktree (a real git worktree, not a mock), and
//! never in the main checkout.

use super::{restore_session, resumable_sessions};
use crate::commands::setup::setup_app_from_config;
use crate::config::{Config, LoadedConfig};
use crate::session::pool::SessionPool;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use std::path::Path;

const AGENT_SESSION_ID: &str = "11111111-2222-3333-4444-555555555555";

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git must be on PATH")
        .success();
    assert!(ok, "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "ci@example.com"]);
    git(dir, &["config", "user.name", "CI"]);
    std::fs::write(dir.join("README.md"), "init").unwrap();
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

fn issue_session(n: u64) -> Session {
    Session::new(
        format!("implement #{n}"),
        "opus".into(),
        "orchestrator".into(),
        Some(n),
        None,
    )
}

fn walk(session: &mut Session, steps: &[SessionStatus]) {
    for &to in steps {
        session
            .transition_to(to, TransitionReason::Spawned)
            .unwrap();
    }
}

#[tokio::test]
async fn resume_restores_quit_paused_session() {
    use SessionStatus::{Running, Spawning, Stalled};
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1266.json");
    let worktrees = || Box::new(GitWorktreeManager::new(repo.path().to_path_buf()));

    // A run with a running agent, a stalled one and one parked at the gate.
    // The agent command does not exist: nothing here may spawn an agent.
    let config: Config = toml::from_str(
        "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n[notifications]\n\
         [agents]\ndefault = \"q\"\n[agents.q]\nkind = \"qwen\"\n\
         command = \"maestro-missing-agent-1266\"\n",
    )
    .unwrap();
    let loaded = LoadedConfig {
        config,
        path: state_dir.path().join("maestro.toml"),
    };
    let store = StateStore::new(store_path.clone());
    let mut app = setup_app_from_config(loaded, store, worktrees(), None);
    let (running, stalled, parked) = (
        issue_session(1266),
        issue_session(1267),
        issue_session(1268),
    );
    let ids = [running.id, stalled.id, parked.id];
    for s in [running, stalled, parked] {
        app.pool.enqueue(s);
    }
    let _ = app.pool.try_promote();
    let kept: Vec<_> = (ids.iter())
        .map(|id| {
            app.pool
                .get_active_mut(*id)
                .unwrap()
                .worktree_path
                .clone()
                .unwrap()
        })
        .collect();
    std::fs::write(kept[0].join("work.txt"), "uncommitted").unwrap();
    let first = app.pool.get_active_mut(ids[0]).unwrap();
    walk(&mut first.session, &[Spawning, Running]);
    assert!(first.session.bind_agent_session_id(AGENT_SESSION_ID));
    walk(
        &mut app.pool.get_active_mut(ids[1]).unwrap().session,
        &[Spawning, Running, Stalled],
    );

    // The confirmed quit.
    crate::tui::save_on_exit(&mut app);
    drop(app);

    // `maestro resume` reads exactly what that quit wrote.
    let state = StateStore::new(store_path).load().unwrap();
    let picked = resumable_sessions(&state, None);
    assert_eq!(picked.len(), 3, "every saved non-terminal session resumes");
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    let restored: Vec<Session> = (picked.iter())
        .map(|s| restore_session(s, &probe).expect("restorable"))
        .collect();
    let first = restored.iter().find(|s| s.id == ids[0]).unwrap();
    assert_eq!(first.agent_session_id.as_deref(), Some(AGENT_SESSION_ID));

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut pool = SessionPool::new(3, worktrees(), tx);
    for s in restored {
        pool.enqueue(s);
    }
    let _ = pool.try_promote();
    for (id, path) in ids.iter().zip(&kept) {
        let managed = pool.get_active_mut(*id).expect("restored with its id");
        assert_eq!(
            managed.worktree_path.as_ref(),
            Some(path),
            "a resumed session must run in its own kept worktree, never the main checkout"
        );
    }
    assert!(kept[0].join("work.txt").exists(), "the kept work survives");
}

#[test]
fn resume_refuses_an_unrecorded_kept_worktree() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    crate::session::worktree::WorktreeManager::create(&probe, "issue-1269").unwrap();
    let saved = issue_session(1269); // worktree_path never recorded

    let refused = restore_session(&saved, &probe).expect_err("must not fall back to cwd");
    assert!(refused.contains("issue-1269"), "{refused}");
}
