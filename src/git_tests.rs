//! Test module for `src/git.rs`. Lives in a sibling file so the
//! production module stays under the 400-LOC cap. Included into
//! `src/git.rs` via `#[cfg(test)] #[path = "git_tests.rs"] mod tests;`.

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

// --- Issue #562: WIP backup commit / amend / detect tests ---

// Group 1: MockGitOps unit tests

#[test]
fn mock_backup_wip_records_call() {
    let ops = MockGitOps::new();
    ops.backup_wip(Path::new("/tmp/wt-562a"), 562).unwrap();
    let calls = ops.backup_wip_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, Path::new("/tmp/wt-562a"));
    assert_eq!(calls[0].1, 562u64);
}

#[test]
fn mock_backup_wip_propagates_should_fail() {
    let ops = MockGitOps::new().with_failure();
    assert!(ops.backup_wip(Path::new("/tmp"), 1).is_err());
}

#[test]
fn mock_head_is_wip_backup_returns_false_by_default() {
    let ops = MockGitOps::new();
    assert!(!ops.head_is_wip_backup(Path::new("/tmp")).unwrap());
}

#[test]
fn mock_head_is_wip_backup_returns_configured_true() {
    let ops = MockGitOps::new().with_head_wip(true);
    assert!(ops.head_is_wip_backup(Path::new("/tmp")).unwrap());
}

#[test]
fn mock_head_is_wip_backup_propagates_should_fail() {
    let ops = MockGitOps::new().with_failure();
    assert!(ops.head_is_wip_backup(Path::new("/tmp")).is_err());
}

#[test]
fn mock_amend_clean_and_push_records_call() {
    let ops = MockGitOps::new();
    ops.amend_clean_and_push(
        Path::new("/tmp/wt-562b"),
        "feat/issue-562",
        "feat: implement #562",
    )
    .unwrap();
    let calls = ops.amend_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, Path::new("/tmp/wt-562b"));
    assert_eq!(calls[0].1, "feat/issue-562");
    assert_eq!(calls[0].2, "feat: implement #562");
}

#[test]
fn mock_amend_clean_and_push_propagates_should_fail() {
    let ops = MockGitOps::new().with_failure();
    assert!(
        ops.amend_clean_and_push(Path::new("/tmp"), "b", "m")
            .is_err()
    );
}

// Group 2: CliGitOps integration tests (real git in tempdir)

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

#[test]
fn cli_backup_wip_creates_commit_with_canonical_subject() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::write(tmp.path().join("model.rs"), "fn f() {}").unwrap();

    let ops = CliGitOps;
    ops.backup_wip(tmp.path(), 562).unwrap();

    let subject = git_head_subject(tmp.path());
    let expected = format!("{}{}{}", WIP_SUBJECT_PREFIX, 562, WIP_SUBJECT_SUFFIX);
    assert_eq!(subject, expected);
    assert!(
        subject.contains("[skip ci]"),
        "WIP subject must contain [skip ci] (AC #5)"
    );
}

#[test]
fn cli_backup_wip_works_with_zero_changes() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let ops = CliGitOps;
    let result = ops.backup_wip(tmp.path(), 562);
    assert!(
        result.is_ok(),
        "backup_wip must --allow-empty: {:?}",
        result.err()
    );
    let subject = git_head_subject(tmp.path());
    assert!(subject.starts_with(WIP_SUBJECT_PREFIX));
}

#[test]
fn cli_head_is_wip_backup_true_after_backup_wip() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let ops = CliGitOps;
    ops.backup_wip(tmp.path(), 42).unwrap();

    assert!(ops.head_is_wip_backup(tmp.path()).unwrap());
}

#[test]
fn cli_head_is_wip_backup_false_on_non_wip_head() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    // HEAD subject is "init" — not a WIP backup commit.

    let ops = CliGitOps;
    assert!(!ops.head_is_wip_backup(tmp.path()).unwrap());
}

#[test]
fn cli_head_is_wip_backup_returns_false_when_worktree_missing() {
    // AC #9: missing worktree must NOT panic and must NOT bubble Err
    // for the detection path — there's simply no WIP at HEAD.
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");

    let ops = CliGitOps;
    let result = ops.head_is_wip_backup(&missing);
    assert!(matches!(result, Ok(false)), "got {:?}", result);
}

#[test]
fn cli_backup_wip_bails_when_worktree_missing() {
    // AC #9: backup_wip must surface a clean error rather than
    // panicking when the worktree path doesn't exist.
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");

    let ops = CliGitOps;
    let result = ops.backup_wip(&missing, 562);
    assert!(result.is_err());
}

#[test]
fn cli_amend_clean_and_push_replaces_wip_subject_at_head() {
    // Build a bare remote + working clone so push works without
    // touching the developer's real origin.
    let remote_tmp = tempfile::tempdir().unwrap();
    let s = std::process::Command::new("git")
        .args(["init", "--bare", "-q", "-b", "main"])
        .current_dir(remote_tmp.path())
        .status()
        .unwrap();
    assert!(s.success());

    let work_tmp = tempfile::tempdir().unwrap();
    let s = std::process::Command::new("git")
        .args([
            "clone",
            "-q",
            remote_tmp.path().to_str().unwrap(),
            work_tmp.path().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(s.success());

    run_git_in(work_tmp.path(), &["config", "user.email", "ci@example.com"]);
    run_git_in(work_tmp.path(), &["config", "user.name", "CI"]);

    std::fs::write(work_tmp.path().join("README.md"), "init").unwrap();
    run_git_in(work_tmp.path(), &["add", "README.md"]);
    run_git_in(work_tmp.path(), &["commit", "-q", "-m", "init"]);
    run_git_in(work_tmp.path(), &["push", "-u", "origin", "main"]);

    let branch = "feat/issue-562";
    run_git_in(work_tmp.path(), &["checkout", "-q", "-b", branch]);
    run_git_in(work_tmp.path(), &["push", "-u", "origin", branch]);

    let ops = CliGitOps;
    ops.backup_wip(work_tmp.path(), 562).unwrap();
    assert!(ops.head_is_wip_backup(work_tmp.path()).unwrap());

    let clean = "feat: implement changes for issue #562";
    ops.amend_clean_and_push(work_tmp.path(), branch, clean)
        .unwrap();

    let subject = git_head_subject(work_tmp.path());
    assert_eq!(
        subject, clean,
        "amend must replace WIP subject with clean message"
    );
    assert!(
        !ops.head_is_wip_backup(work_tmp.path()).unwrap(),
        "head_is_wip_backup must be false after amend"
    );
    let log = std::process::Command::new("git")
        .args(["log", "--oneline", "origin/feat/issue-562"])
        .current_dir(work_tmp.path())
        .output()
        .unwrap();
    let log_text = String::from_utf8_lossy(&log.stdout);
    assert!(
        !log_text.contains("WIP:"),
        "remote must not contain WIP commit after amend+push"
    );
}
