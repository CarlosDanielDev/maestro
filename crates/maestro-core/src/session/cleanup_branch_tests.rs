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
    tracing::subscriber::with_default(subscriber, || {
        // Plain `cargo test` runs many tests in one process: a test on
        // another thread can register the kept-branch callsite while no
        // subscriber is live and cache `Interest::never`, so this scoped
        // subscriber never sees the event. Rebuild with it live.
        tracing::callsite::rebuild_interest_cache();
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
