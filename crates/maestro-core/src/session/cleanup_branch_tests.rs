//! #1473: orphan cleanup keeps a `maestro/<slug>` branch that holds commits
//! no other ref has. Real git repos in a tempdir. Sibling file via `#[path]`
//! so `cleanup.rs` stays under the 400-line gate cap.

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be on PATH");
    assert!(
        out.status.success(),
        "git {args:?} failed in {dir:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .current_dir(repo)
        .output()
        .expect("git must be on PATH")
        .status
        .success()
}

fn commit_in(dir: &Path) {
    git(dir, &["commit", "-q", "--allow-empty", "-m", "work"]);
}

/// `<tmp>/repo` with one commit on `main` and a session worktree at
/// `.maestro/worktrees/issue-7` on branch `maestro/issue-7`, as the pool
/// makes it.
fn repo_with_worktree() -> (tempfile::TempDir, PathBuf, OrphanWorktree) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).expect("mkdir repo");
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "ci@example.com"],
        &["config", "user.name", "CI"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        git(&repo, args);
    }
    let path = repo.join(".maestro").join("worktrees").join("issue-7");
    let path_arg = path.to_str().expect("utf-8 tempdir path");
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "maestro/issue-7", path_arg],
    );
    let orphan = OrphanWorktree {
        path,
        name: "issue-7".to_string(),
    };
    (tmp, repo, orphan)
}

#[test]
fn orphan_cleanup_keeps_branch_with_unique_commits() {
    let (_tmp, repo, orphan) = repo_with_worktree();
    commit_in(&orphan.path);

    CleanupManager::new(&repo)
        .remove_orphan(&orphan)
        .expect("orphan cleanup must succeed");

    assert!(!orphan.path.exists(), "the worktree dir is still removed");
    assert!(
        branch_exists(&repo, "maestro/issue-7"),
        "a branch with a commit no other ref has must survive orphan cleanup"
    );
}

/// The commit is pushed to a real (bare) remote, so only the
/// remote-tracking ref also holds it: dropping `--remotes` from the
/// unique-commit count would keep this branch and fail the test.
#[test]
fn orphan_cleanup_removes_branch_without_unique_commits() {
    let (tmp, repo, orphan) = repo_with_worktree();
    let remote = tmp.path().join("remote.git");
    let remote_arg = remote.to_str().expect("utf-8 tempdir path");
    git(tmp.path(), &["init", "-q", "--bare", remote_arg]);
    git(&repo, &["remote", "add", "origin", remote_arg]);
    commit_in(&orphan.path);
    git(&orphan.path, &["push", "-q", "origin", "maestro/issue-7"]);

    CleanupManager::new(&repo)
        .remove_orphan(&orphan)
        .expect("orphan cleanup must succeed");

    assert!(!orphan.path.exists(), "the worktree dir is removed");
    assert!(
        !branch_exists(&repo, "maestro/issue-7"),
        "a branch whose commits live on the remote must be removed"
    );
}

#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("log buffer lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn orphan_cleanup_logs_kept_branch() {
    let (_tmp, repo, orphan) = repo_with_worktree();
    commit_in(&orphan.path);
    commit_in(&orphan.path);

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // Plain `cargo test` runs many tests in one process. With only one live
    // dispatcher, tracing-core rebuilds callsite interest from the calling
    // thread's own default, so a test on another thread that first hits the
    // kept-branch callsite caches `Interest::never` for every thread and
    // this scoped subscriber misses the event. A second live dispatcher
    // makes every rebuild ask all live dispatchers, this one included.
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    tracing::subscriber::with_default(subscriber, || {
        CleanupManager::new(&repo)
            .remove_orphan(&orphan)
            .expect("orphan cleanup must succeed");
    });

    let log =
        String::from_utf8(captured.0.lock().expect("log buffer lock").clone()).expect("utf-8 log");
    let line = log
        .lines()
        .find(|l| l.contains("orphan cleanup kept branch"))
        .unwrap_or_else(|| panic!("no kept-branch log line: {log}"));
    assert!(
        line.contains("maestro/issue-7"),
        "branch name missing: {line}"
    );
    assert!(
        line.contains("unique_commits=2"),
        "commit count missing: {line}"
    );
}

/// #1483: the report lists the branch kept for its unique commits, with
/// the count, and leaves out the branch that was deleted.
#[test]
fn cleanup_report_lists_kept_branches() {
    let (_tmp, repo, kept) = repo_with_worktree();
    commit_in(&kept.path);
    commit_in(&kept.path);
    let path = repo.join(".maestro").join("worktrees").join("issue-8");
    let path_arg = path.to_str().expect("utf-8 tempdir path");
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "maestro/issue-8", path_arg],
    );
    let gone = OrphanWorktree {
        path,
        name: "issue-8".to_string(),
    };

    let report = CleanupManager::new(&repo)
        .remove_orphans(&[kept, gone])
        .expect("orphan cleanup must succeed");

    assert_eq!(report.removed, 2, "both worktrees are removed");
    assert_eq!(
        report.kept_branches,
        vec![KeptBranch {
            name: "maestro/issue-7".to_string(),
            unique_commits: Some(2),
        }],
        "only the branch with unique commits is listed as kept"
    );
    assert!(!branch_exists(&repo, "maestro/issue-8"));
}

/// #1483: a branch already gone before cleanup is not listed as kept
/// ("count unknown" would be a false report).
#[test]
fn cleanup_report_skips_a_branch_that_no_longer_exists() {
    let (_tmp, repo, orphan) = repo_with_worktree();
    git(
        &repo,
        &[
            "worktree",
            "remove",
            "--force",
            orphan.path.to_str().unwrap(),
        ],
    );
    git(&repo, &["branch", "-d", "maestro/issue-7"]);

    let report = CleanupManager::new(&repo)
        .remove_orphans(&[orphan])
        .expect("orphan cleanup must succeed");

    assert_eq!(report.removed, 1, "an already-gone orphan is a success");
    assert!(
        report.kept_branches.is_empty(),
        "a missing branch must not be reported as kept: {:?}",
        report.kept_branches
    );
}

/// Judge round on PR #1551: only `rev-parse` exit 1 means "gone". A git
/// error (exit 128 here: not a repo) must list the branch as kept, count
/// unknown, never hide it.
#[test]
fn kept_branch_lists_the_branch_when_git_fails_with_another_code() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".maestro").join("worktrees")).expect("mkdir");
    let orphan = OrphanWorktree {
        path: tmp.path().join(".maestro/worktrees/issue-7"),
        name: "issue-7".to_string(),
    };

    let kept = CleanupManager::new(tmp.path()).kept_branch(&orphan);

    assert_eq!(
        kept,
        Some(KeptBranch {
            name: "maestro/issue-7".to_string(),
            unique_commits: None,
        })
    );
}

/// Same rule when git cannot even start (its cwd does not exist).
#[test]
fn kept_branch_lists_the_branch_when_git_cannot_start() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let orphan = OrphanWorktree {
        path: tmp.path().join("missing/issue-7"),
        name: "issue-7".to_string(),
    };

    let kept = CleanupManager::new(&tmp.path().join("missing")).kept_branch(&orphan);

    assert_eq!(
        kept.map(|k| k.unique_commits),
        Some(None),
        "a spawn error must list the branch, count unknown"
    );
}
