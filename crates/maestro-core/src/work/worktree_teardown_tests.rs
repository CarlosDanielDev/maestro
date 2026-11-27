//! Unit tests for `worktree_teardown`. Sibling file via `#[path]` so the
//! module stays under the 400-line gate cap (#1484).

use super::*;

#[test]
fn is_unsafe_root_slash_is_unsafe() {
    assert!(is_unsafe_root(Path::new("/"), None));
}

#[test]
fn is_unsafe_root_home_is_unsafe() {
    let home = Path::new("/home/user");
    assert!(is_unsafe_root(home, Some(home)));
}

#[test]
fn is_unsafe_root_normal_root_is_safe() {
    assert!(!is_unsafe_root(
        Path::new("/tmp/maestro/worktrees"),
        Some(Path::new("/home/user"))
    ));
}

#[test]
fn is_unsafe_root_home_subdir_is_safe() {
    // A subdir of HOME is fine — only HOME *exactly* is unsafe.
    assert!(!is_unsafe_root(
        Path::new("/home/user/projects/foo"),
        Some(Path::new("/home/user"))
    ));
}

#[test]
fn is_worktree_absent_matches_known_phrase() {
    assert!(is_worktree_absent("fatal: '/tmp/wt' is not a working tree"));
}

#[test]
fn is_worktree_absent_false_for_other_errors() {
    assert!(!is_worktree_absent("fatal: not a git repository"));
}

#[test]
fn is_branch_absent_matches_known_phrase() {
    assert!(is_branch_absent("error: branch 'feat/issue-740' not found"));
}

#[test]
fn is_branch_absent_false_for_other_errors() {
    assert!(!is_branch_absent(
        "error: Cannot delete branch 'main' checked out at '/repo'"
    ));
}

#[test]
fn sanity_check_out_of_root_when_path_escapes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("root");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&root).expect("mkdir root");
    std::fs::create_dir_all(&outside).expect("mkdir outside");

    let result = sanity_check_under_root(&outside, &root);
    assert!(
        matches!(result, Err(TeardownError::OutOfRoot { .. })),
        "expected OutOfRoot, got {result:?}"
    );
}

#[test]
fn sanity_check_refuses_empty_root() {
    // The interaction cwd-fallback derives an empty root; it must be
    // refused, never canonicalized against cwd (#741 sec).
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("wt");
    std::fs::create_dir_all(&path).expect("mkdir wt");
    let result = sanity_check_under_root(&path, Path::new(""));
    assert!(
        matches!(result, Err(TeardownError::UnsafeRoot(_))),
        "expected UnsafeRoot for empty root, got {result:?}"
    );
}

#[test]
fn sanity_check_refuses_relative_root() {
    let result = sanity_check_under_root(Path::new("."), Path::new(".maestro/worktrees"));
    assert!(
        matches!(result, Err(TeardownError::UnsafeRoot(_))),
        "expected UnsafeRoot for relative root, got {result:?}"
    );
}

#[test]
fn sanity_check_none_when_path_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("does_not_exist");
    let result = sanity_check_under_root(&path, tmp.path());
    assert!(
        matches!(result, Ok(None)),
        "expected Ok(None), got {result:?}"
    );
}

#[test]
fn sanity_check_some_when_path_inside_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("wt-1");
    std::fs::create_dir_all(&path).expect("mkdir wt");

    let result = sanity_check_under_root(&path, tmp.path());
    match result {
        Ok(Some(canon)) => {
            let canon_root = tmp.path().canonicalize().expect("canon root");
            assert!(
                canon.starts_with(&canon_root),
                "returned path {canon:?} must be under root {canon_root:?}"
            );
        }
        other => panic!("expected Ok(Some(_)), got {other:?}"),
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn branch_present(repo: &Path, branch: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .current_dir(repo)
        .output()
        .expect("git must be on PATH")
        .status
        .success()
}

/// A repo with one commit on `main` and `maestro/x` holding `unique`
/// commits no other ref has.
fn repo_with_branch(unique: usize) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "ci@example.com"],
        &["config", "user.name", "CI"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "init"],
        &["checkout", "-q", "-b", "maestro/x"],
    ] {
        git(dir, args);
    }
    for _ in 0..unique {
        git(dir, &["commit", "-q", "--allow-empty", "-m", "work"]);
    }
    git(dir, &["checkout", "-q", "main"]);
    tmp
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

/// Runs `delete_branch(.., keep = true)` and returns what it logged.
fn delete_kept(repo: &Path) -> String {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // A second live dispatcher so plain `cargo test` cannot cache
    // `Interest::never` for these callsites from another thread (#1479).
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    tracing::subscriber::with_default(subscriber, || {
        delete_branch(repo, "maestro/x", true).expect("delete_branch must succeed");
    });
    String::from_utf8(captured.0.lock().expect("log buffer lock").clone()).expect("utf-8 log")
}

#[test]
fn delete_branch_logs_truthful_reason() {
    // Some(n): the branch holds work only it has.
    let repo = repo_with_branch(2);
    let log = delete_kept(repo.path());
    assert!(
        log.contains("keeping branch with 2 unique commit(s)"),
        "unique case: {log}"
    );
    assert!(branch_present(repo.path(), "maestro/x"), "unique: kept");

    // None, branch present: its commit object is gone, so git cannot count.
    let repo = repo_with_branch(1);
    let sha = git(repo.path(), &["rev-parse", "maestro/x"]);
    let object = repo
        .path()
        .join(".git/objects")
        .join(&sha[..2])
        .join(&sha[2..]);
    std::fs::remove_file(&object).expect("remove commit object");
    let log = delete_kept(repo.path());
    assert!(
        log.contains("could not count commits; keeping it to be safe"),
        "uncountable case: {log}"
    );
    assert!(
        branch_present(repo.path(), "maestro/x"),
        "uncountable: kept"
    );

    // Branch missing: nothing to delete, and no claim that it was kept.
    let repo = repo_with_branch(0);
    git(repo.path(), &["branch", "-d", "maestro/x"]);
    let log = delete_kept(repo.path());
    assert!(
        log.contains("branch already absent; nothing to delete"),
        "missing case: {log}"
    );
    assert!(
        !log.contains("keeping"),
        "missing case claims a keep: {log}"
    );
    assert!(
        !branch_present(repo.path(), "maestro/x"),
        "missing: still absent"
    );

    // Some(0): unchanged, the branch is deleted.
    let repo = repo_with_branch(0);
    delete_kept(repo.path());
    assert!(
        !branch_present(repo.path(), "maestro/x"),
        "no unique commits: deleted"
    );
}

/// #1481: teardown whose unique-commit count times out keeps the branch
/// (the safe side, unchanged rule). A FIFO no one writes to as the
/// object-store alternates file makes `git rev-list` block on `open`
/// until it is killed, while `worktree remove` and `rev-parse --verify`
/// (they read no objects) still answer: real git, no sleep.
#[cfg(unix)]
#[test]
fn teardown_keeps_branch_when_count_times_out() {
    let repo = repo_with_branch(0);
    let root = repo.path().join(".maestro/worktrees");
    let path = root.join("x");
    let path_arg = path.to_str().expect("utf-8 tempdir");
    git(repo.path(), &["worktree", "add", "-q", path_arg, "maestro/x"]);
    let alternates = repo.path().join(".git/objects/info/alternates");
    std::fs::create_dir_all(alternates.parent().expect("parent")).expect("mkdir info");
    let made = Command::new("mkfifo").arg(&alternates).status();
    assert!(made.expect("mkfifo").success(), "mkfifo");

    let (wt, wt_root) = (path.clone(), root.clone());
    let wiped = crate::work::git_within::within_or_fail(
        std::time::Duration::from_secs(10),
        &alternates,
        move || wipe(1481, &wt, "maestro/x", &wt_root, true).is_ok(),
    );
    assert!(wiped, "teardown must succeed");
    assert!(!path.exists(), "the worktree is removed");
    std::fs::remove_file(&alternates).expect("remove fifo");
    assert!(
        branch_present(repo.path(), "maestro/x"),
        "a branch whose count timed out is kept"
    );
}
