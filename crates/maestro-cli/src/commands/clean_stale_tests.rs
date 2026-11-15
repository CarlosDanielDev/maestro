//! Tests for `maestro clean --stale` (#1149). Separate file so
//! `clean_stale.rs` stays inside the 400-line cap.

use super::*;
use crate::session::types::Session;

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git")
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Real git repo with one empty commit so `git worktree add` works.
fn init_temp_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "test"]);
    run_git(dir.path(), &["commit", "--allow-empty", "-q", "-m", "init"]);
    dir
}

/// Register a real worktree and save a state fixture whose only session
/// is `Running`, owned by a process that no longer exists.
fn seed_stale_session(dir: &Path, state_path: &Path, wt: &Path) {
    run_git(
        dir,
        &[
            "worktree",
            "add",
            "-b",
            "maestro/session-1149aaa",
            wt.to_str().expect("utf8 worktree path"),
        ],
    );
    let dead = crate::system::process::testing::dead_process_pid();
    let mut s = Session::new(
        "stale work".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1149),
        None,
    );
    s.status = SessionStatus::Running;
    s.owner_pid = Some(dead);
    s.worktree_path = Some(wt.to_path_buf());
    let mut state = MaestroState::default();
    state.sessions.push(s);
    StateStore::new(state_path.to_path_buf())
        .save(&state)
        .expect("save state fixture");
}

#[test]
fn clean_stale_removes_stale_worktree_and_marks_session_killed() {
    let dir = init_temp_repo();
    let wt_root = dir.path().join(".maestro/worktrees");
    std::fs::create_dir_all(&wt_root).unwrap();
    let wt = wt_root.join("session-1149aaa");
    let state_path = dir.path().join("maestro-state.json");
    seed_stale_session(dir.path(), &state_path, &wt);

    let report = clean_stale(dir.path(), &state_path, false).expect("clean_stale");

    assert_eq!(report.stale_total, 1);
    assert_eq!(report.found.len(), 1);
    assert_eq!(report.removed, 1);
    assert_eq!(report.marked_killed, 1);
    assert!(!wt.exists(), "stale worktree must be removed");
    let listed = git(dir.path(), &["worktree", "list"]);
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(
        !stdout.contains("session-1149aaa"),
        "worktree must be unregistered; got:\n{stdout}"
    );
    let after = StateStore::new(state_path).load().expect("reload state");
    assert_eq!(after.sessions[0].status, SessionStatus::Killed);
    assert!(
        after.sessions[0].finished_at.is_some(),
        "terminal move must stamp finished_at"
    );
}

#[test]
fn clean_stale_dry_run_lists_without_touching_anything() {
    let dir = init_temp_repo();
    let wt_root = dir.path().join(".maestro/worktrees");
    std::fs::create_dir_all(&wt_root).unwrap();
    let wt = wt_root.join("session-1149aaa");
    let state_path = dir.path().join("maestro-state.json");
    seed_stale_session(dir.path(), &state_path, &wt);

    let report = clean_stale(dir.path(), &state_path, true).expect("clean_stale");

    assert_eq!(report.stale_total, 1);
    assert_eq!(report.found.len(), 1);
    assert_eq!(report.removed, 0);
    assert_eq!(report.marked_killed, 0);
    assert!(wt.exists(), "dry run must not remove the worktree");
    let after = StateStore::new(state_path).load().expect("reload state");
    assert_eq!(
        after.sessions[0].status,
        SessionStatus::Running,
        "dry run must not mutate the state file"
    );
}

#[test]
fn clean_stale_never_touches_a_worktree_owned_by_a_live_session() {
    // Judge round on #1177: stale record A (issue 42, dead owner) shares
    // the slug `issue-42` with a LIVE session whose worktree was created
    // after A died. Removing by derived path would force-remove the live
    // session's worktree and branch.
    let dir = init_temp_repo();
    let wt_root = dir.path().join(".maestro/worktrees");
    std::fs::create_dir_all(&wt_root).unwrap();
    let wt = wt_root.join("issue-42");
    run_git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-b",
            "maestro/issue-42",
            wt.to_str().expect("utf8 worktree path"),
        ],
    );

    let dead = crate::system::process::testing::dead_process_pid();
    let mut stale_a = Session::new(
        "old".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(42),
        None,
    );
    stale_a.status = SessionStatus::Running;
    stale_a.owner_pid = Some(dead);

    let mut live = Session::new(
        "new".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(42),
        None,
    );
    live.status = SessionStatus::Running;
    live.owner_pid = Some(std::process::id());

    let mut state = MaestroState::default();
    state.sessions = vec![stale_a, live];
    let state_path = dir.path().join("maestro-state.json");
    StateStore::new(state_path.to_path_buf())
        .save(&state)
        .expect("save state fixture");

    let report = clean_stale(dir.path(), &state_path, false).expect("clean_stale");

    assert_eq!(report.stale_total, 1);
    assert!(
        wt.exists(),
        "the live session's worktree must survive clean --stale"
    );
    let listed = git(dir.path(), &["worktree", "list"]);
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(
        stdout.contains("issue-42"),
        "worktree must stay registered; got:\n{stdout}"
    );
    assert_eq!(report.removed, 0, "nothing may be removed on a conflict");
    assert_eq!(report.conflicts.len(), 1, "the collision must be reported");
    assert_eq!(
        report.marked_killed, 1,
        "the dead record itself still closes out — only its worktree is protected"
    );
}

#[test]
fn mark_stale_sessions_killed_only_touches_stale_sessions() {
    let dead = crate::system::process::testing::dead_process_pid();
    let mut stale = Session::new("a".into(), "opus".into(), "orchestrator".into(), None, None);
    stale.status = SessionStatus::Running;
    stale.owner_pid = Some(dead);
    let mut live = Session::new("b".into(), "opus".into(), "orchestrator".into(), None, None);
    live.status = SessionStatus::Running;
    live.owner_pid = Some(std::process::id());
    let mut done = Session::new("c".into(), "opus".into(), "orchestrator".into(), None, None);
    done.status = SessionStatus::Completed;
    done.owner_pid = Some(dead);

    let mut state = MaestroState::default();
    state.sessions = vec![stale, live, done];

    let marked = mark_stale_sessions_killed(&mut state, |_| false);

    assert_eq!(marked, 1, "only the dead-owner in-flight session is marked");
    assert_eq!(state.sessions[0].status, SessionStatus::Killed);
    assert_eq!(
        state.sessions[1].status,
        SessionStatus::Running,
        "live-owner session must stay untouched"
    );
    assert_eq!(
        state.sessions[2].status,
        SessionStatus::Completed,
        "terminal session must stay untouched"
    );
}

#[test]
fn mark_stale_sessions_killed_skips_sessions_whose_worktree_removal_failed() {
    // Failed wipe keeps the session stale so `clean --stale` can retry;
    // only sessions whose removal succeeded (or that had no worktree)
    // are closed out.
    let dead = crate::system::process::testing::dead_process_pid();
    let mut a = Session::new("a".into(), "opus".into(), "orchestrator".into(), None, None);
    a.status = SessionStatus::Running;
    a.owner_pid = Some(dead);
    a.worktree_path = Some(std::path::PathBuf::from("/wt/a-wt"));
    let mut b = Session::new("b".into(), "opus".into(), "orchestrator".into(), None, None);
    b.status = SessionStatus::Running;
    b.owner_pid = Some(dead);
    b.worktree_path = Some(std::path::PathBuf::from("/wt/b-wt"));

    let mut state = MaestroState::default();
    state.sessions = vec![a, b];

    let marked = mark_stale_sessions_killed(&mut state, |s| {
        s.worktree_path
            .as_deref()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "a-wt")
    });

    assert_eq!(marked, 1, "only the non-failed session is marked");
    assert_eq!(
        state.sessions[0].status,
        SessionStatus::Running,
        "failed-removal session must stay stale for retry"
    );
    assert_eq!(state.sessions[1].status, SessionStatus::Killed);
}
