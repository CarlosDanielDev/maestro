//! Test module for `src/git.rs`. Lives in a sibling file so the
//! production module stays under the 400-LOC cap. Included into
//! `src/git.rs` via `#[cfg(test)] #[path = "git_tests.rs"] mod tests;`.
//! WIP-backup lifecycle tests (#562, #1143) live in the child module
//! `wip` (`git_wip_tests.rs`) and reach these helpers via `super::`.

use super::*;

#[test]
fn mock_git_ops_succeeds_by_default() {
    let ops = MockGitOps::new();
    assert!(
        ops.commit_and_push(Path::new("/tmp"), "main", "test")
            .is_ok()
    );
}

#[test]
fn mock_git_ops_fails_when_configured() {
    let ops = MockGitOps::new().with_failure();
    assert!(
        ops.commit_and_push(Path::new("/tmp"), "main", "test")
            .is_err()
    );
}

// --- Issue #159: list_remote_branches tests ---

#[test]
fn mock_git_ops_list_remote_branches_filters_by_prefix() {
    let ops = MockGitOps {
        remote_branches: vec![
            "origin/maestro/issue-42".to_string(),
            "origin/maestro/issue-99".to_string(),
            "origin/feat/something".to_string(),
        ],
        ..MockGitOps::new()
    };
    let branches = ops.list_remote_branches("maestro/issue-").unwrap();
    assert_eq!(branches.len(), 2);
    assert!(branches.contains(&"origin/maestro/issue-42".to_string()));
    assert!(branches.contains(&"origin/maestro/issue-99".to_string()));
}

#[test]
fn mock_git_ops_list_remote_branches_returns_empty_when_no_match() {
    let ops = MockGitOps {
        remote_branches: vec!["origin/feat/something".to_string()],
        ..MockGitOps::new()
    };
    let branches = ops.list_remote_branches("maestro/issue-").unwrap();
    assert!(branches.is_empty());
}

// --- Issue #514: has_commits_ahead detection ---

#[test]
fn mock_git_ops_has_commits_ahead_returns_false_by_default() {
    let ops = MockGitOps::new();
    assert!(
        !ops.has_commits_ahead(Path::new("/tmp"), "branch", "main")
            .unwrap()
    );
}

#[test]
fn mock_git_ops_has_commits_ahead_returns_configured_value() {
    let ops = MockGitOps::new().with_commits_ahead(true);
    assert!(
        ops.has_commits_ahead(Path::new("/tmp"), "branch", "main")
            .unwrap()
    );
}

#[test]
fn mock_git_ops_has_commits_ahead_propagates_should_fail() {
    let ops = MockGitOps::new().with_failure();
    assert!(
        ops.has_commits_ahead(Path::new("/tmp"), "branch", "main")
            .is_err()
    );
}

fn run_git_in(dir: &std::path::Path, args: &[&str]) {
    let s = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git must be on PATH");
    assert!(s.success(), "git {:?} failed in {:?}", args, dir);
}

fn init_repo(dir: &std::path::Path) {
    run_git_in(dir, &["init", "-q", "-b", "main"]);
    run_git_in(dir, &["config", "user.email", "ci@example.com"]);
    run_git_in(dir, &["config", "user.name", "CI"]);
    std::fs::write(dir.join("README.md"), "init").unwrap();
    run_git_in(dir, &["add", "README.md"]);
    run_git_in(dir, &["commit", "-q", "-m", "init"]);
}

fn git_head_subject(dir: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .args(["log", "-1", "--pretty=%s"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `git rev-list --count <rev>` in `dir` (#1143 commit-count asserts).
fn git_rev_count(dir: &std::path::Path, rev: &str) -> u32 {
    let out = std::process::Command::new("git")
        .args(["rev-list", "--count", rev])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

// --- Issue #1142: changed_files_against_merge_base ---

#[test]
fn cli_changed_files_lists_merge_base_diff_repo_relative() {
    // #1142 repro shape: a config file committed on base (the agent will
    // only ever Read it) plus the agent's new file on the branch. Only
    // the branch-side file is in the diff.
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::write(tmp.path().join("pyproject.toml"), "[project]").unwrap();
    run_git_in(tmp.path(), &["add", "pyproject.toml"]);
    run_git_in(tmp.path(), &["commit", "-q", "-m", "config"]);

    run_git_in(tmp.path(), &["checkout", "-q", "-b", "maestro/issue-3"]);
    std::fs::write(tmp.path().join("stub.txt"), "x").unwrap();
    run_git_in(tmp.path(), &["add", "stub.txt"]);
    run_git_in(tmp.path(), &["commit", "-q", "-m", "feat: stub"]);

    let ops = CliGitOps;
    let files = ops
        .changed_files_against_merge_base(tmp.path(), "main")
        .unwrap();
    assert_eq!(
        files,
        vec!["stub.txt".to_string()],
        "must list exactly the branch-side diff, repo-relative"
    );
}

#[test]
fn cli_changed_files_empty_when_branch_equals_base() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let ops = CliGitOps;
    let files = ops
        .changed_files_against_merge_base(tmp.path(), "main")
        .unwrap();
    assert!(
        files.is_empty(),
        "no commits ahead = empty diff, got {:?}",
        files
    );
}

#[test]
fn cli_changed_files_rejects_dash_prefixed_base() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let ops = CliGitOps;
    assert!(
        ops.changed_files_against_merge_base(tmp.path(), "--upload-pack")
            .is_err()
    );
}

#[test]
fn mock_changed_files_returns_configured_and_propagates_failure() {
    let ops = MockGitOps::new().with_changed_files(vec!["stub.txt".into()]);
    assert_eq!(
        ops.changed_files_against_merge_base(Path::new("/tmp"), "main")
            .unwrap(),
        vec!["stub.txt".to_string()]
    );
    assert!(
        MockGitOps::new()
            .with_failure()
            .changed_files_against_merge_base(Path::new("/tmp"), "main")
            .is_err()
    );
}

// WIP backup lifecycle (#562 / #1143): child module, sibling file.
#[path = "git_wip_tests.rs"]
mod wip;
