//! WIP backup lifecycle tests for `src/git.rs`: `backup_wip`,
//! `amend_clean_and_push`, and `head_is_wip_backup` (#562) plus the
//! clean-tree skip guard (#1143). Lives in a sibling file, included as
//! a child of the `tests` module in `git_tests.rs`, so both files stay
//! under the 400-LOC cap. `use super::*` reaches the git helpers
//! (`init_repo`, `run_git_in`, `git_head_subject`, `git_rev_count`);
//! `use crate::git::*` reaches the ops and WIP subject constants.

use super::*;
use crate::git::*;

// --- Issue #562: MockGitOps backup/amend/detect unit tests ---

#[test]
fn mock_backup_wip_records_call() {
    let ops = MockGitOps::new();
    let created = ops.backup_wip(Path::new("/tmp/wt-562a"), 562).unwrap();
    assert!(
        created,
        "mock default must report a WIP commit created (dirty tree)"
    );
    let (len, path, issue) = {
        let calls = ops.backup_wip_calls.lock().unwrap();
        (calls.len(), calls[0].0.clone(), calls[0].1)
    };
    assert_eq!(len, 1);
    assert_eq!(path, Path::new("/tmp/wt-562a"));
    assert_eq!(issue, 562u64);
}

#[test]
fn mock_backup_wip_reports_skip_when_configured() {
    // #1143: clean-tree plumbing — the mock can report "no commit
    // created" so the pipeline tests can exercise the skip path.
    let ops = MockGitOps::new().with_backup_skips();
    let created = ops.backup_wip(Path::new("/tmp"), 1).unwrap();
    assert!(!created, "configured skip must report no commit created");
    assert_eq!(ops.backup_wip_calls.lock().unwrap().len(), 1);
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
    let (len, path, branch, message) = {
        let calls = ops.amend_calls.lock().unwrap();
        (
            calls.len(),
            calls[0].0.clone(),
            calls[0].1.clone(),
            calls[0].2.clone(),
        )
    };
    assert_eq!(len, 1);
    assert_eq!(path, Path::new("/tmp/wt-562b"));
    assert_eq!(branch, "feat/issue-562");
    assert_eq!(message, "feat: implement #562");
}

#[test]
fn mock_amend_clean_and_push_propagates_should_fail() {
    let ops = MockGitOps::new().with_failure();
    assert!(
        ops.amend_clean_and_push(Path::new("/tmp"), "b", "m")
            .is_err()
    );
}

// --- Issue #562 / #1143: CliGitOps tests (real git in tempdir) ---

/// Bare remote + working clone, `main` committed and pushed, identity
/// configured. Returns the remote tempdir so it outlives the asserts.
fn init_repo_with_remote(work: &std::path::Path) -> tempfile::TempDir {
    let remote_tmp = tempfile::tempdir().unwrap();
    let s = std::process::Command::new("git")
        .args(["init", "--bare", "-q", "-b", "main"])
        .current_dir(remote_tmp.path())
        .status()
        .unwrap();
    assert!(s.success());

    let s = std::process::Command::new("git")
        .args([
            "clone",
            "-q",
            remote_tmp.path().to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(s.success());

    run_git_in(work, &["config", "user.email", "ci@example.com"]);
    run_git_in(work, &["config", "user.name", "CI"]);
    std::fs::write(work.join("README.md"), "init").unwrap();
    run_git_in(work, &["add", "README.md"]);
    run_git_in(work, &["commit", "-q", "-m", "init"]);
    run_git_in(work, &["push", "-u", "origin", "main"]);
    remote_tmp
}

fn head_subject_on(dir: &std::path::Path, rev: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["log", "-1", "--pretty=%s", rev])
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
    let created = ops.backup_wip(tmp.path(), 562).unwrap();
    assert!(created, "dirty tree must report a WIP commit created");

    let subject = git_head_subject(tmp.path());
    let expected = format!("{}{}{}", WIP_SUBJECT_PREFIX, 562, WIP_SUBJECT_SUFFIX);
    assert_eq!(subject, expected);
    assert!(
        subject.contains("[skip ci]"),
        "WIP subject must contain [skip ci] (AC #5)"
    );
}

#[test]
fn cli_backup_wip_skips_commit_when_tree_clean() {
    // #1143: the agent already committed its own work — a clean tree
    // must NOT get an `--allow-empty` WIP commit, which the post-gate
    // amend would otherwise push as an empty `feat: implement changes
    // for issue #N` head commit.
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let ops = CliGitOps;
    let created = ops.backup_wip(tmp.path(), 562).unwrap();
    assert!(!created, "clean tree must skip the WIP backup commit");

    assert_eq!(git_head_subject(tmp.path()), "init");
    assert_eq!(
        git_rev_count(tmp.path(), "HEAD"),
        1,
        "no commit may be added to a clean tree"
    );
}

#[test]
fn cli_head_is_wip_backup_true_after_backup_wip() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::write(tmp.path().join("model.rs"), "fn f() {}").unwrap();

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
    let work_tmp = tempfile::tempdir().unwrap();
    let remote_tmp = init_repo_with_remote(work_tmp.path());

    let branch = "feat/issue-562";
    run_git_in(work_tmp.path(), &["checkout", "-q", "-b", branch]);
    run_git_in(work_tmp.path(), &["push", "-u", "origin", branch]);

    // #1143 AC: uncommitted agent work is captured by the WIP backup,
    // then rewritten into ONE clean conventional commit by the amend.
    std::fs::write(work_tmp.path().join("model.rs"), "fn f() {}").unwrap();

    let ops = CliGitOps;
    let created = ops.backup_wip(work_tmp.path(), 562).unwrap();
    assert!(created, "dirty tree must create the WIP backup commit");
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
    assert_eq!(
        git_rev_count(work_tmp.path(), "origin/main..origin/feat/issue-562"),
        1,
        "amend must yield exactly one commit ahead of base"
    );
    let show = std::process::Command::new("git")
        .args(["show", "--name-only", "--pretty=%s", "HEAD"])
        .current_dir(work_tmp.path())
        .output()
        .unwrap();
    let show_text = String::from_utf8_lossy(&show.stdout);
    assert!(
        show_text.contains("model.rs"),
        "the clean commit must contain the agent's changes"
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

#[test]
fn cli_clean_tree_pushes_only_agent_commit() {
    // #1143 AC: clean worktree before gates (the agent committed its own
    // work) — the pushed branch must carry ONLY the agent's commit, not
    // an empty `feat: implement changes for issue #N` head commit.
    let work_tmp = tempfile::tempdir().unwrap();
    let remote_tmp = init_repo_with_remote(work_tmp.path());

    // The stub agent's own commit, exactly like the #1143 repro.
    let branch = "maestro/issue-3";
    run_git_in(work_tmp.path(), &["checkout", "-q", "-b", branch]);
    std::fs::write(work_tmp.path().join("stub.txt"), "stub\n").unwrap();
    run_git_in(work_tmp.path(), &["add", "stub.txt"]);
    run_git_in(
        work_tmp.path(),
        &["commit", "-q", "-m", "stub: add stub.txt"],
    );

    let ops = CliGitOps;
    let created = ops.backup_wip(work_tmp.path(), 3).unwrap();
    assert!(!created, "clean tree must not create a WIP backup commit");

    // Post-gate path with head_is_wip == false: push-only fallback.
    ops.commit_and_push(
        work_tmp.path(),
        branch,
        "feat: implement changes for issue #3",
    )
    .unwrap();

    assert_eq!(
        git_rev_count(work_tmp.path(), "origin/main..origin/maestro/issue-3"),
        1,
        "branch must carry exactly the agent's commit"
    );
    assert_eq!(
        head_subject_on(work_tmp.path(), "origin/maestro/issue-3"),
        "stub: add stub.txt",
        "remote branch head must be the agent's commit, not an empty conventional commit"
    );
    let log = std::process::Command::new("git")
        .args(["log", "--format=%s", "origin/main..origin/maestro/issue-3"])
        .current_dir(work_tmp.path())
        .output()
        .unwrap();
    let log_text = String::from_utf8_lossy(&log.stdout);
    assert!(
        !log_text.contains("feat: implement"),
        "no empty conventional head commit may exist on the pushed branch"
    );
}

#[test]
fn cli_gate_changes_commit_cleanly_when_no_wip() {
    // #1143 AC: clean tree before gates → skip; changes that APPEAR
    // during gates (e.g. accepted snapshots) must land in one clean
    // conventional commit via the commit_and_push fallback.
    let work_tmp = tempfile::tempdir().unwrap();
    let remote_tmp = init_repo_with_remote(work_tmp.path());

    // Tree is clean — no agent commit, no uncommitted work.
    let ops = CliGitOps;
    let created = ops.backup_wip(work_tmp.path(), 4).unwrap();
    assert!(!created, "clean tree must skip the WIP backup commit");

    // Gate artifact appears after the backup point.
    std::fs::write(work_tmp.path().join("snapshot.trycmd"), "ok").unwrap();

    let branch = "maestro/issue-4";
    run_git_in(work_tmp.path(), &["checkout", "-q", "-b", branch]);
    ops.commit_and_push(
        work_tmp.path(),
        branch,
        "feat: implement changes for issue #4",
    )
    .unwrap();

    assert_eq!(
        git_rev_count(work_tmp.path(), "origin/main..origin/maestro/issue-4"),
        1,
        "gate changes must land in exactly one commit"
    );
    assert_eq!(
        head_subject_on(work_tmp.path(), "origin/maestro/issue-4"),
        "feat: implement changes for issue #4",
        "gate changes must be committed with the clean conventional message"
    );
    let show = std::process::Command::new("git")
        .args([
            "show",
            "--name-only",
            "--pretty=%s",
            "origin/maestro/issue-4",
        ])
        .current_dir(work_tmp.path())
        .output()
        .unwrap();
    let show_text = String::from_utf8_lossy(&show.stdout);
    assert!(
        show_text.contains("snapshot.trycmd"),
        "the commit must contain the gate-produced changes"
    );
}
