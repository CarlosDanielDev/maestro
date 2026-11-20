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
    crate::tui::save_on_exit(&mut app).expect("saved");
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

/// #1266 judge round 3: a session the cap stopped (Errored, BudgetCap)
/// survives a plain launch in between, and `maestro resume` still restores
/// it into its own kept worktree.
#[tokio::test]
async fn cap_stop_survives_a_plain_launch_and_resumes() {
    use SessionStatus::{Running, Spawning};
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1266.json");
    let worktrees = || Box::new(GitWorktreeManager::new(repo.path().to_path_buf()));
    let app_for = |path: &Path| {
        let config: Config = toml::from_str(
            "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n[notifications]\n\
             [agents]\ndefault = \"q\"\n[agents.q]\nkind = \"qwen\"\n\
             command = \"maestro-missing-agent-1266\"\n",
        )
        .unwrap();
        let loaded = LoadedConfig {
            config,
            path: path.join("maestro.toml"),
        };
        setup_app_from_config(
            loaded,
            StateStore::new(store_path.clone()),
            worktrees(),
            None,
        )
    };

    // Run 1: the cap stops a pid-less agent, then the operator quits.
    let mut app = app_for(state_dir.path());
    let stopped = issue_session(1270);
    let id = stopped.id;
    app.pool.enqueue(stopped);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).unwrap();
    let kept = managed.worktree_path.clone().unwrap();
    std::fs::write(kept.join("work.txt"), "uncommitted").unwrap();
    walk(&mut managed.session, &[Spawning, Running]);
    let out = app.pool.pause_all(TransitionReason::BudgetCap);
    assert_eq!(out.stopped, 1, "setup: no pid, so the cap stops it");
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    // Run 2: a plain launch, as `maestro` / `maestro run` set it up.
    let mut app = app_for(state_dir.path());
    let saved = app.state.sessions.clone();
    app.pool.keep_saved_resumable(&saved);
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);

    // `maestro resume`.
    let state = StateStore::new(store_path).load().unwrap();
    let picked = resumable_sessions(&state, None);
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    let s = picked
        .iter()
        .find(|s| s.id == id)
        .expect("the cap-stopped session is still saved");
    let restored = restore_session(s, &probe).expect("restorable");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut pool = SessionPool::new(1, worktrees(), tx);
    pool.enqueue(restored);
    let _ = pool.try_promote();
    let managed = pool.get_active_mut(id).expect("restored with its id");
    assert_eq!(managed.worktree_path.as_ref(), Some(&kept));
    assert!(kept.join("work.txt").exists(), "the kept work survives");
}

/// #1423: what a quit saved for an Interactive, a CiFix and a ConflictFix
/// session comes back from `maestro resume` in its own kept worktree, bound
/// to its saved agent session id, still that kind of session.
#[tokio::test]
async fn resume_restores_interactive_and_fix_sessions() {
    use crate::session::types::{CiFixContext, ConflictFixContext, SessionMode};
    use SessionStatus::{CiFix, Completed, ConflictFix, Running, Spawning};
    const IDS: [&str; 3] = [
        "aaaaaaaa-0000-0000-0000-000000001423",
        "bbbbbbbb-0000-0000-0000-000000001423",
        "cccccccc-0000-0000-0000-000000001423",
    ];
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let state_dir = tempfile::tempdir().unwrap();
    let store_path = state_dir.path().join("state-1423.json");
    let worktrees = || Box::new(GitWorktreeManager::new(repo.path().to_path_buf()));
    let config: Config = toml::from_str(
        "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n[notifications]\n\
         [agents]\ndefault = \"q\"\n[agents.q]\nkind = \"qwen\"\n\
         command = \"maestro-missing-agent-1423\"\n",
    )
    .unwrap();
    let loaded = LoadedConfig {
        config,
        path: state_dir.path().join("maestro.toml"),
    };
    let mut app = setup_app_from_config(
        loaded,
        StateStore::new(store_path.clone()),
        worktrees(),
        None,
    );

    let mut chat = issue_session(1423);
    chat.session_mode = SessionMode::Interactive;
    chat.agent_id = Some("q".into());
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
    let ids = [chat.id, ci.id, conflict.id];
    for s in [chat, ci, conflict] {
        app.pool.enqueue(s);
    }
    let _ = app.pool.try_promote();
    let walks: [&[SessionStatus]; 3] = [&[Spawning, Running, Completed], &[CiFix], &[ConflictFix]];
    let mut kept = Vec::new();
    for ((id, steps), agent_id) in ids.iter().zip(walks).zip(IDS) {
        let managed = app.pool.get_active_mut(*id).unwrap();
        walk(&mut managed.session, steps);
        assert!(managed.session.bind_agent_session_id(agent_id));
        kept.push(managed.worktree_path.clone().unwrap());
    }
    std::fs::write(kept[0].join("chat.txt"), "uncommitted").unwrap();

    // The confirmed quit, then `maestro resume` on exactly what it wrote.
    crate::tui::save_on_exit(&mut app).expect("saved");
    drop(app);
    let state = StateStore::new(store_path).load().unwrap();
    let picked = resumable_sessions(&state, None);
    assert_eq!(picked.len(), 3, "all three kinds resume");
    let probe = GitWorktreeManager::new(repo.path().to_path_buf());
    let restored: Vec<Session> = (picked.iter())
        .map(|s| restore_session(s, &probe).expect("restorable"))
        .collect();
    let get = |id| restored.iter().find(|s| s.id == id).unwrap();
    for (id, agent_id) in ids.iter().zip(IDS) {
        assert_eq!(get(*id).agent_session_id.as_deref(), Some(agent_id));
    }
    assert_eq!(get(ids[0]).session_mode, SessionMode::Interactive);
    assert_eq!(get(ids[0]).agent_id.as_deref(), Some("q"), "same agent");
    assert!(get(ids[1]).ci_fix_context.is_some(), "still a CI fix");
    assert!(
        get(ids[2]).conflict_fix_context.is_some(),
        "still a conflict fix"
    );

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
    assert!(kept[0].join("chat.txt").exists(), "the kept work survives");
}

/// #1423: an adopted conversation is never picked up by `maestro resume`,
/// even when a crash left it saved live: it runs in its transcript's cwd,
/// and resume would run it in a worktree instead.
#[test]
fn resume_skips_an_adopted_conversation() {
    let mut state = crate::state::types::MaestroState::default();
    let mut adopted = issue_session(1426);
    adopted.session_mode = crate::session::types::SessionMode::Interactive;
    adopted.adopted_cwd = Some(std::env::temp_dir());
    walk(
        &mut adopted,
        &[SessionStatus::Spawning, SessionStatus::Running],
    );
    state.sessions.push(adopted);
    assert!(resumable_sessions(&state, None).is_empty());
}
