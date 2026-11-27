//! Commits only one branch holds (#1469): deleting that branch would lose
//! them for good. Session teardown and the Kill dialog both read this count,
//! so what the dialog says is what teardown does.

use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::git_within::{GIT_COUNT_TIMEOUT, output_within};
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
/// (no such branch, not a repo) or does not answer in
/// [`GIT_COUNT_TIMEOUT`] (#1481): callers must then treat the branch as
/// holding work. No network: only local refs are read.
pub fn unique_commit_count(repo_dir: &Path, branch: &str) -> Option<usize> {
    // `--exclude` drops this branch from the next `--branches` only, so
    // every other ref still counts as "the commit lives elsewhere too".
    let mut git = Command::new("git");
    git.arg("rev-list")
        .arg("--count")
        .arg(format!("refs/heads/{branch}"))
        .arg("--not")
        .arg(format!("--exclude={branch}"))
        .args(["--branches", "--remotes", "--tags"])
        .current_dir(repo_dir);
    count_within(git, GIT_COUNT_TIMEOUT)
}

/// The number `command` prints, when it answers with success in `timeout`.
fn count_within(command: Command, timeout: Duration) -> Option<usize> {
    output_within(command, timeout, |mut out| {
        let mut text = String::new();
        out.read_to_string(&mut text).ok()?;
        text.trim().parse().ok()
    })
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

    /// #1481: a git that never answers. The count gives up at the timeout,
    /// so the Kill dialog still opens (count unknown) and teardown keeps
    /// the branch. The stuck runner is `git hash-object` on a FIFO no one
    /// writes to, as in #1471: its `open` blocks until git is killed, so
    /// the test needs no sleep.
    #[cfg(unix)]
    #[test]
    fn unique_count_times_out() {
        let repo = repo();
        let fifo = repo.path().join("never-written-1481");
        let made = Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(made.success(), "mkfifo");
        let mut stuck = Command::new("git");
        stuck.arg("hash-object").arg(&fifo).current_dir(repo.path());
        let started = std::time::Instant::now();
        let count =
            crate::work::git_within::within_or_fail(Duration::from_secs(5), &fifo, move || {
                count_within(stuck, Duration::from_millis(50))
            });
        assert_eq!(count, None, "a git that does not answer is unknown");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the count does not wait for git"
        );
    }
}
