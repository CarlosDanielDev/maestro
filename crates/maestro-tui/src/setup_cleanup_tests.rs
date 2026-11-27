//! #1552: startup cleanup hands over the branches it kept instead of
//! dropping the report. Real git repo in a tempdir.

use std::path::Path;
use std::process::Command;

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

/// An orphaned session worktree whose branch holds one unique commit: the
/// worktree goes, the branch is kept and handed back with its count.
#[test]
fn startup_cleanup_reports_kept_branches() {
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
    let wt = repo.join(".maestro").join("worktrees").join("issue-7");
    let wt_arg = wt.to_str().expect("utf-8 tempdir path");
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "maestro/issue-7", wt_arg],
    );
    git(&wt, &["commit", "-q", "--allow-empty", "-m", "work"]);

    let kept = startup_cleanup(&repo);

    assert!(!wt.exists(), "the orphaned worktree is removed");
    assert_eq!(
        kept,
        vec![KeptBranch {
            name: "maestro/issue-7".to_string(),
            unique_commits: Some(1),
        }],
        "startup cleanup must hand over the kept branch"
    );
}
