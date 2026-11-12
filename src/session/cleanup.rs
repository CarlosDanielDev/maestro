use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::session::pool::session_slug;
use crate::session::types::Session;
use crate::state::types::MaestroState;
use crate::work::worktree_teardown::wipe_worktree;

/// Manages cleanup of orphaned worktrees left by crashed processes.
pub struct CleanupManager {
    worktree_dir: PathBuf,
}

/// Info about an orphan worktree found during scan.
#[derive(Debug, Clone, PartialEq)]
pub struct OrphanWorktree {
    pub path: PathBuf,
    pub name: String,
}

/// Worktrees of stale sessions (#1149), split by safety.
#[derive(Debug, Default, PartialEq)]
pub struct StaleScan {
    /// Safe to remove: no live session owns this slug.
    pub safe: Vec<OrphanWorktree>,
    /// Removal refused: the dir-name is now owned by a live (non-stale,
    /// non-terminal) session. After a crash, a re-run may fail worktree
    /// creation against the dead session's leftover, later release the
    /// slug, and hand it to a fresh LIVE session — removing "the stale
    /// session's worktree" then would tear down that live session's
    /// worktree and branch (judge round on PR #1177).
    pub conflicts: Vec<OrphanWorktree>,
}

impl CleanupManager {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            worktree_dir: repo_root.join(".maestro").join("worktrees"),
        }
    }

    /// Scan for orphaned worktree directories that are not tracked by git.
    pub fn scan_orphans(&self) -> Result<Vec<OrphanWorktree>> {
        if !self.worktree_dir.exists() {
            return Ok(Vec::new());
        }

        // Get list of git-tracked worktrees
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let tracked_paths: HashSet<PathBuf> = stdout
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .map(PathBuf::from)
            .collect();

        // List entries in .maestro/worktrees/
        let mut orphans = Vec::new();
        for entry in std::fs::read_dir(&self.worktree_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !tracked_paths.iter().any(|tp| {
                    let tp_canonical = tp.canonicalize().unwrap_or_else(|_| tp.clone());
                    tp_canonical == canonical
                }) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    orphans.push(OrphanWorktree { path, name });
                }
            }
        }

        Ok(orphans)
    }

    /// Candidate worktree (path, dir-name) for a stale session: the slug
    /// the pool used at creation ([`session_slug`]), falling back to the
    /// recorded `Session::worktree_path` — whose primary meaning is the
    /// #560 FailedGates retention marker, which crash-killed sessions
    /// never carry. `None` when nothing exists on disk.
    pub fn stale_candidate_path(&self, session: &Session) -> Option<(PathBuf, String)> {
        let derived = self.worktree_dir.join(session_slug(session));
        let path = if derived.exists() {
            derived
        } else {
            let p = session.worktree_path.as_deref().filter(|p| p.exists())?;
            p.to_path_buf()
        };
        let name = path.file_name()?.to_string_lossy().to_string();
        Some((path, name))
    }

    /// Collect the worktrees of stale sessions (#1149): sessions still
    /// claiming in-flight work whose owning maestro process is gone. A
    /// candidate whose dir-name matches the slug of a live session is
    /// returned as a conflict and must not be removed.
    pub fn stale_worktrees(&self, state: &MaestroState) -> StaleScan {
        let live_slugs: HashSet<String> = state
            .sessions
            .iter()
            .filter(|s| !s.status.is_terminal() && !s.is_stale())
            .map(session_slug)
            .collect();

        let mut scan = StaleScan::default();
        for candidate in state
            .sessions
            .iter()
            .filter(|s| s.is_stale())
            .filter_map(|s| {
                self.stale_candidate_path(s)
                    .map(|(path, name)| OrphanWorktree { path, name })
            })
        {
            let owned = candidate
                .path
                .file_name()
                .is_some_and(|n| live_slugs.contains(&n.to_string_lossy().to_string()));
            if owned {
                scan.conflicts.push(candidate);
            } else {
                scan.safe.push(candidate);
            }
        }
        scan
    }

    /// Remove orphaned worktrees. Returns the count removed.
    ///
    /// #938: each removal routes through the hardened [`wipe_worktree`]
    /// primitive (rooted at `self.worktree_dir`), so it inherits the root
    /// sanity-check (refuses `/` / `$HOME`, canonicalized containment) and the
    /// leading-dash injection guard. The previous raw `git worktree remove`
    /// call and the `std::fs::remove_dir_all` fallback — neither root-gated —
    /// are gone. A refused or failed orphan is logged and skipped (best-effort,
    /// no longer aborts the whole batch on the first failure).
    pub fn remove_orphans(&self, orphans: &[OrphanWorktree]) -> Result<usize> {
        let mut removed = 0;
        for orphan in orphans {
            if self.remove_orphan(orphan).is_ok() {
                removed += 1;
            } else {
                tracing::warn!(
                    "orphan worktree teardown refused/failed for {}",
                    orphan.name
                );
            }
        }
        Ok(removed)
    }

    /// Remove one orphaned/stale worktree. Per-item form of
    /// [`Self::remove_orphans`] so callers can react to individual
    /// failures (#1149: a session whose worktree wipe failed stays stale
    /// for retry instead of being marked `Killed` blind). Same #938
    /// hardening as the batch form — the branch is `maestro/<name>` and
    /// [`wipe_worktree`] is idempotent, so a missing branch is treated as
    /// success.
    pub fn remove_orphan(&self, orphan: &OrphanWorktree) -> Result<()> {
        let branch = format!("maestro/{}", orphan.name);
        wipe_worktree(0, &orphan.path, &branch, &self.worktree_dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionStatus;

    // --- Issue #1149: stale session worktrees ---

    #[test]
    fn stale_worktrees_collects_only_dead_owner_sessions_with_existing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let wt_root = dir.path().join(".maestro/worktrees");
        std::fs::create_dir_all(&wt_root).unwrap();

        let dead = crate::system::process::testing::dead_process_pid();

        // Stale session found by slug derivation alone — the crash-killed
        // case: no `worktree_path` was ever persisted, but the pool-named
        // dir exists under the worktree root (#1149).
        let mut derived =
            Session::new("d".into(), "opus".into(), "orchestrator".into(), None, None);
        derived.status = SessionStatus::Running;
        derived.owner_pid = Some(dead);
        let derived_dir = wt_root.join(session_slug(&derived));
        std::fs::create_dir_all(&derived_dir).unwrap();

        // Stale session found via a recorded path (no slug-named dir).
        let mut recorded =
            Session::new("r".into(), "opus".into(), "orchestrator".into(), None, None);
        recorded.status = SessionStatus::Running;
        recorded.owner_pid = Some(dead);
        let recorded_dir = wt_root.join("custom-retained");
        std::fs::create_dir_all(&recorded_dir).unwrap();
        recorded.worktree_path = Some(recorded_dir.clone());

        // Live owner: never collected.
        let mut live = Session::new("l".into(), "opus".into(), "orchestrator".into(), None, None);
        live.status = SessionStatus::Running;
        live.owner_pid = Some(std::process::id());
        let live_dir = wt_root.join(session_slug(&live));
        std::fs::create_dir_all(&live_dir).unwrap();

        // Stale but its recorded worktree is already gone from disk.
        let mut gone = Session::new("g".into(), "opus".into(), "orchestrator".into(), None, None);
        gone.status = SessionStatus::Running;
        gone.owner_pid = Some(dead);
        gone.worktree_path = Some(wt_root.join("session-gone0"));

        // Stale but no candidate path exists anywhere.
        let mut no_wt = Session::new("n".into(), "opus".into(), "orchestrator".into(), None, None);
        no_wt.status = SessionStatus::Running;
        no_wt.owner_pid = Some(dead);

        let mut state = MaestroState::default();
        state.sessions = vec![derived, recorded, live, gone, no_wt];

        let mgr = CleanupManager::new(dir.path());
        let scan = mgr.stale_worktrees(&state);

        assert!(
            scan.conflicts.is_empty(),
            "no slug overlap in this fixture, got: {:?}",
            scan.conflicts
        );
        assert_eq!(
            scan.safe.len(),
            2,
            "only dead-owner sessions with an existing candidate path are collected"
        );
        assert!(
            scan.safe.iter().any(|o| o.path == derived_dir
                && o.name == derived_dir.file_name().unwrap().to_string_lossy()),
            "slug-derived worktree must be found, got: {:?}",
            scan.safe
        );
        assert!(
            scan.safe
                .iter()
                .any(|o| o.path == recorded_dir && o.name == "custom-retained"),
            "recorded-path worktree must be found, got: {:?}",
            scan.safe
        );
    }

    #[test]
    fn stale_worktrees_marks_live_owned_slugs_as_conflicts() {
        // Judge round on PR #1177: stale record and live session share the
        // slug `issue-42` — the stale record's derived path IS the live
        // session's worktree, so removal must be refused.
        let dir = tempfile::tempdir().unwrap();
        let wt_root = dir.path().join(".maestro/worktrees");
        std::fs::create_dir_all(&wt_root).unwrap();
        let wt = wt_root.join("issue-42");
        std::fs::create_dir_all(&wt).unwrap();

        let dead = crate::system::process::testing::dead_process_pid();
        let mut stale = Session::new(
            "old".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(42),
            None,
        );
        stale.status = SessionStatus::Running;
        stale.owner_pid = Some(dead);

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
        state.sessions = vec![stale, live];

        let mgr = CleanupManager::new(dir.path());
        let scan = mgr.stale_worktrees(&state);

        assert!(
            scan.safe.is_empty(),
            "conflicting candidate must not be safe, got: {:?}",
            scan.safe
        );
        assert_eq!(scan.conflicts.len(), 1);
        assert_eq!(scan.conflicts[0].path, wt);
        assert_eq!(scan.conflicts[0].name, "issue-42");
    }

    #[test]
    fn new_creates_manager_with_correct_path() {
        let mgr = CleanupManager::new(Path::new("/tmp/repo"));
        assert_eq!(
            mgr.worktree_dir,
            PathBuf::from("/tmp/repo/.maestro/worktrees")
        );
    }

    #[test]
    fn scan_orphans_returns_empty_when_dir_missing() {
        let mgr = CleanupManager::new(Path::new("/tmp/nonexistent-repo-12345"));
        let orphans = mgr.scan_orphans().unwrap();
        assert!(orphans.is_empty());
    }

    // #938: a misconfigured worktree root of `/` must be refused before any
    // destructive call. We construct the manager with `worktree_dir = "/"`
    // directly (same module → private field is reachable) and assert the orphan
    // is NOT counted as removed — `wipe_worktree`'s `UnsafeRoot` guard fires
    // before it ever shells out to git or touches the filesystem.
    #[test]
    fn remove_orphans_refuses_unsafe_root() {
        let mgr = CleanupManager {
            worktree_dir: PathBuf::from("/"),
        };
        let orphan = OrphanWorktree {
            path: PathBuf::from("/etc"),
            name: "etc".to_string(),
        };
        let removed = mgr.remove_orphans(&[orphan]).expect("must not error");
        assert_eq!(
            removed, 0,
            "an unsafe `/` root must refuse, removing nothing"
        );
        assert!(
            Path::new("/etc").exists(),
            "the guard must run before any destructive call — /etc stays put"
        );
    }
}
