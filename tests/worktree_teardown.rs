//! Integration tests for `wipe_worktree` (issue #740).
//!
//! External integration tests reaching the `pub` surface of
//! `maestro::work::{wipe_worktree, TeardownError}`. These spawn a real `git`
//! and build real worktrees under a `tempfile::tempdir`, so they exercise the
//! destructive path end-to-end (not mocked).
//!
//! `git` must be on `PATH`; CI provides it.

use maestro::work::worktree_teardown::{TeardownError, wipe_worktree};
use std::path::Path;
use std::process::Command;

/// Run a git command in `dir`, asserting it succeeds. Used only for fixture
/// setup — the code under test does its own error handling.
fn git_ok(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be spawnable");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `git init` a repo with one commit on `main` so `git worktree add` works.
fn init_repo(dir: &Path) {
    git_ok(dir, &["init", "-q", "-b", "main"]);
    git_ok(dir, &["config", "user.email", "test@example.com"]);
    git_ok(dir, &["config", "user.name", "Test"]);
    git_ok(dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
}

/// True when `git branch --list <branch>` in `dir` is empty (branch gone).
fn branch_absent(dir: &Path, branch: &str) -> bool {
    let output = Command::new("git")
        .args(["branch", "--list", branch])
        .current_dir(dir)
        .output()
        .expect("git branch --list must spawn");
    String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

const BRANCH: &str = "feat/issue-740";

#[test]
fn happy_path_removes_worktree_and_branch() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    let wt = tmp.path().join("wt");
    git_ok(
        tmp.path(),
        &["worktree", "add", wt.to_str().unwrap(), "-b", BRANCH],
    );
    assert!(wt.exists(), "pre-condition: worktree dir exists");

    let result = wipe_worktree(740, &wt, BRANCH, tmp.path());

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert!(!wt.exists(), "worktree dir must be gone");
    assert!(branch_absent(tmp.path(), BRANCH), "branch must be deleted");
}

#[test]
fn idempotent_second_call_returns_ok() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    let wt = tmp.path().join("wt");
    git_ok(
        tmp.path(),
        &["worktree", "add", wt.to_str().unwrap(), "-b", BRANCH],
    );

    wipe_worktree(740, &wt, BRANCH, tmp.path()).expect("first call succeeds");
    let second = wipe_worktree(740, &wt, BRANCH, tmp.path());

    assert!(
        second.is_ok(),
        "second call must be idempotent, got {second:?}"
    );
}

#[test]
fn uncommitted_file_is_force_removed() {
    // A destructive primitive must force-remove even a dirty worktree.
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    let wt = tmp.path().join("wt");
    git_ok(
        tmp.path(),
        &["worktree", "add", wt.to_str().unwrap(), "-b", BRANCH],
    );
    std::fs::write(wt.join("dirty.txt"), b"uncommitted").expect("write dirty file");

    let result = wipe_worktree(740, &wt, BRANCH, tmp.path());

    assert!(
        result.is_ok(),
        "force removal of dirty worktree must succeed, got {result:?}"
    );
    assert!(!wt.exists(), "dirty worktree dir must be gone");
}

#[test]
fn out_of_root_path_is_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("root");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&root).expect("mkdir root");
    std::fs::create_dir_all(&outside).expect("mkdir outside");

    let result = wipe_worktree(740, &outside, BRANCH, &root);

    assert!(
        matches!(result, Err(TeardownError::OutOfRoot { .. })),
        "expected OutOfRoot, got {result:?}"
    );
    assert!(outside.exists(), "rejected path must be left untouched");
}

#[test]
fn unsafe_root_slash_is_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let wt = tmp.path().join("wt");

    // root = "/" must be refused even though the path under it is missing —
    // the unsafe-root guard runs before the missing-path idempotency skip.
    let result = wipe_worktree(740, &wt, BRANCH, Path::new("/"));

    assert!(
        matches!(result, Err(TeardownError::UnsafeRoot(_))),
        "expected UnsafeRoot, got {result:?}"
    );
}

#[test]
fn symlink_escape_is_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("root");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&root).expect("mkdir root");
    std::fs::create_dir_all(&outside).expect("mkdir outside");

    // A symlink *inside* the root that points outside it. Canonicalize resolves
    // it to `outside`, so the containment check must reject it.
    let link = root.join("escape");
    std::os::unix::fs::symlink(&outside, &link).expect("create symlink");

    let result = wipe_worktree(740, &link, BRANCH, &root);

    assert!(
        matches!(result, Err(TeardownError::OutOfRoot { .. })),
        "expected OutOfRoot for symlink escape, got {result:?}"
    );
}

#[test]
fn missing_branch_is_ok() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    let wt = tmp.path().join("wt");
    git_ok(
        tmp.path(),
        &["worktree", "add", wt.to_str().unwrap(), "-b", BRANCH],
    );
    // Remove worktree + branch out-of-band; wipe_worktree must still return Ok.
    git_ok(
        tmp.path(),
        &["worktree", "remove", "--force", wt.to_str().unwrap()],
    );
    git_ok(tmp.path(), &["branch", "-D", BRANCH]);

    let result = wipe_worktree(740, &wt, BRANCH, tmp.path());

    assert!(result.is_ok(), "missing branch must be Ok, got {result:?}");
}

#[test]
fn leading_dash_branch_is_treated_as_literal() {
    // Security regression (#740): a branch value beginning with `-` must be
    // passed as a literal name, never re-parsed by git as a flag. Without the
    // `--` end-of-options terminator, `git branch -D -r` means "delete
    // remote-tracking branches", not "delete the branch named -r".
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    let wt = tmp.path().join("wt");
    git_ok(
        tmp.path(),
        &["worktree", "add", wt.to_str().unwrap(), "-b", BRANCH],
    );
    // An unrelated branch that must survive the teardown.
    git_ok(tmp.path(), &["branch", "keep-me"]);

    let result = wipe_worktree(740, &wt, "-r", tmp.path());

    assert!(
        result.is_ok(),
        "leading-dash branch must be treated as a literal (absent) name, got {result:?}"
    );
    assert!(
        !branch_absent(tmp.path(), "keep-me"),
        "unrelated branch must survive a leading-dash branch teardown"
    );
}

#[test]
fn missing_path_is_ok() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());
    let wt = tmp.path().join("never-existed");
    assert!(!wt.exists(), "pre-condition: path is absent");

    let result = wipe_worktree(740, &wt, BRANCH, tmp.path());

    assert!(result.is_ok(), "missing path must be Ok, got {result:?}");
}
