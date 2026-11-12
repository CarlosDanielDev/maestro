use crate::session::cleanup::{CleanupManager, OrphanWorktree};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use std::path::Path;

pub fn cmd_clean(dry_run: bool, stale: bool) -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;

    if stale {
        let report = clean_stale(&repo_root, &StateStore::default_path(), dry_run)?;
        print_stale_clean_summary(&report, dry_run);
        return Ok(());
    }

    let mgr = CleanupManager::new(&repo_root);
    let orphans = mgr.scan_orphans()?;

    if orphans.is_empty() {
        println!("No orphaned worktrees found.");
        return Ok(());
    }

    println!("Found {} orphaned worktree(s):", orphans.len());
    for orphan in &orphans {
        println!("  {} ({})", orphan.name, orphan.path.display());
    }

    if dry_run {
        println!("\nDry run — no changes made.");
    } else {
        let removed = mgr.remove_orphans(&orphans)?;
        println!("\nRemoved {} worktree(s).", removed);
    }

    Ok(())
}

/// Outcome of one `maestro clean --stale` pass (#1149). Returned so tests
/// can assert on data instead of stdout.
#[derive(Debug)]
pub struct StaleCleanReport {
    /// All stale sessions in the state file, with or without a worktree.
    pub stale_total: usize,
    /// The subset whose worktree still exists on disk.
    pub found: Vec<OrphanWorktree>,
    pub removed: usize,
    pub marked_killed: usize,
}

/// Move every stale session (#1149) to the terminal `Killed` state.
/// User-initiated: `clean --stale` is the explicit act that closes them —
/// `status` never mutates the state file on read. `Killed` is a valid
/// target from every spawn-bearing status, so the transition always
/// succeeds for sessions the [`SessionStatus`]-level staleness predicate
/// selected.
pub fn mark_stale_sessions_killed(state: &mut MaestroState) -> usize {
    let mut marked = 0;
    for session in &mut state.sessions {
        if session.is_stale()
            && session
                .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
                .is_ok()
        {
            marked += 1;
        }
    }
    marked
}

/// `maestro clean --stale` (#1149): list and remove the worktrees of stale
/// sessions (honoring `dry_run`), then close the stale sessions out as
/// `Killed` in the state file. Sessions without a worktree are still
/// marked; there is simply nothing to remove for them.
pub fn clean_stale(
    repo_root: &Path,
    state_path: &Path,
    dry_run: bool,
) -> anyhow::Result<StaleCleanReport> {
    let mgr = CleanupManager::new(repo_root);
    let store = StateStore::new(state_path.to_path_buf());
    let mut state = store.load()?;
    let stale_total = state.sessions.iter().filter(|s| s.is_stale()).count();
    let found = mgr.stale_worktrees(&state);

    if dry_run {
        return Ok(StaleCleanReport {
            stale_total,
            found,
            removed: 0,
            marked_killed: 0,
        });
    }

    let removed = mgr.remove_orphans(&found)?;
    let marked_killed = mark_stale_sessions_killed(&mut state);
    store.save(&state)?;

    Ok(StaleCleanReport {
        stale_total,
        found,
        removed,
        marked_killed,
    })
}

fn print_stale_clean_summary(report: &StaleCleanReport, dry_run: bool) {
    if report.stale_total == 0 {
        println!("No stale sessions found.");
        return;
    }

    if !report.found.is_empty() {
        println!("Found {} stale session worktree(s):", report.found.len());
        for orphan in &report.found {
            println!("  {} ({})", orphan.name, orphan.path.display());
        }
    }
    let without_worktree = report.stale_total - report.found.len();
    if without_worktree > 0 {
        println!("{without_worktree} stale session(s) carry no worktree.");
    }

    if dry_run {
        println!("\nDry run — no changes made.");
    } else {
        println!(
            "\nRemoved {} worktree(s). Marked {} stale session(s) as KILLED.",
            report.removed, report.marked_killed
        );
    }
}

#[cfg(test)]
mod tests {
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

        let marked = mark_stale_sessions_killed(&mut state);

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
}
