//! Commits only one branch holds (#1469): deleting that branch would lose
//! them for good. Session teardown and the Kill dialog both read this count,
//! so what the dialog says is what teardown does.

use std::path::Path;
use std::process::Command;

use super::worktree_teardown::{TeardownError, wipe};

/// Session teardown (Kill and every other `pool.finalize`, #1469): the
/// guarded [`super::worktree_teardown::wipe_worktree`], except the branch is
/// kept when [`unique_commit_count`] is not `Some(0)`. Orphan cleanup uses
/// it too (#1473); the interaction quit (its dialog says unpushed commits
/// are lost) still calls `wipe_worktree`.
pub fn wipe_worktree_keeping_unpushed(
    issue_number: u64,
    path: &Path,
    branch: &str,
    worktree_root: &Path,
) -> Result<(), TeardownError> {
    wipe(issue_number, path, branch, worktree_root, true)
}

/// How many commits `refs/heads/<branch>` holds that no other local branch,
/// remote-tracking branch or tag reaches. Run from any directory inside the
/// repo (a worktree works: refs are shared). `None` when git cannot tell
/// (no such branch, not a repo): callers must then treat the branch as
/// holding work. No network: only local refs are read.
pub fn unique_commit_count(repo_dir: &Path, branch: &str) -> Option<usize> {
    // `--exclude` drops this branch from the next `--branches` only, so
    // every other ref still counts as "the commit lives elsewhere too".
    let out = Command::new("git")
        .arg("rev-list")
        .arg("--count")
        .arg(format!("refs/heads/{branch}"))
        .arg("--not")
        .arg(format!("--exclude={branch}"))
        .args(["--branches", "--remotes", "--tags"])
        .current_dir(repo_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git must be on PATH")
            .status
            .success();
        assert!(ok, "git {args:?} failed in {dir:?}");
    }

    /// A repo with one commit on `main` and a branch `maestro/x` at it.
    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "ci@example.com"],
            &["config", "user.name", "CI"],
            &["commit", "-q", "--allow-empty", "-m", "init"],
            &["branch", "maestro/x"],
        ] {
            git(tmp.path(), args);
        }
        tmp
    }

    #[test]
    fn a_fresh_branch_holds_no_unique_commits() {
        let repo = repo();
        assert_eq!(unique_commit_count(repo.path(), "maestro/x"), Some(0));
    }

    #[test]
    fn a_commit_only_the_branch_has_is_counted() {
        let repo = repo();
        git(repo.path(), &["checkout", "-q", "maestro/x"]);
        git(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", "work"],
        );
        git(repo.path(), &["checkout", "-q", "main"]);
        assert_eq!(unique_commit_count(repo.path(), "maestro/x"), Some(1));
    }

    #[test]
    fn a_missing_branch_is_unknown_not_empty() {
        let repo = repo();
        assert_eq!(unique_commit_count(repo.path(), "maestro/nope"), None);
    }
}
