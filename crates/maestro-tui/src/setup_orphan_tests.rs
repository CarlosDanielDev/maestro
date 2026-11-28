//! #1563: startup reports an untracked dir under `.maestro/worktrees/`
//! and never deletes it. Real git repos in a tempdir; every call passes
//! the repo root, so nothing depends on the process cwd.

use super::*;
use std::path::PathBuf;
use std::time::Instant;

/// `<tmp>/repo`, a git repo, with an untracked `.maestro/worktrees/leftover`
/// holding one file.
pub(crate) fn repo_with_leftover() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).expect("mkdir repo");
    let out = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .output()
        .expect("git must be on PATH");
    assert!(out.status.success(), "git init failed: {out:?}");
    let leftover = repo.join(".maestro").join("worktrees").join("leftover");
    std::fs::create_dir_all(&leftover).expect("mkdir leftover");
    std::fs::write(leftover.join("file"), "only copy").expect("write file");
    (tmp, repo, leftover)
}

const LINE: &str = "untracked dir kept · rm -rf .maestro/worktrees/leftover";

fn visible(app: &App, now: Instant) -> Option<String> {
    (app.toasts.visible(now)).map(|v| format!("{:?} {}", v.severity, v.message))
}

#[test]
fn startup_orphan_untracked_dir_is_reported_not_deleted() {
    let (_tmp, repo, leftover) = repo_with_leftover();
    let mut app = crate::tui::make_test_app("orphan-1563");
    let mut stderr = Vec::new();
    let now = Instant::now();

    report_startup_orphans(&mut app, &repo, now, Some(&mut stderr));

    assert!(leftover.join("file").exists(), "the dir and its file stay");
    assert_eq!(visible(&app, now), Some(format!("Error {LINE}")));
    let stderr = String::from_utf8(stderr).expect("utf-8");
    assert_eq!(stderr, format!("warning: {LINE}\n"), "--once prints it");
}

#[test]
fn orphan_notice_shown_once_and_again_after_recreate() {
    let (_tmp, repo, leftover) = repo_with_leftover();

    assert_eq!(orphan_notices(&repo), [LINE], "first launch");
    assert!(
        orphan_notices(&repo).is_empty(),
        "the shown record stops it"
    );

    std::fs::remove_dir_all(&leftover).expect("removed by hand");
    std::fs::create_dir(&leftover).expect("recreated, same name");
    assert_eq!(orphan_notices(&repo), [LINE], "a new dir is shown again");
}

/// Not a git repo: `git worktree list` fails (#1559). The error is shown
/// and printed, never dropped, and nothing is removed.
#[test]
fn startup_scan_error_is_reported() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let x = tmp.path().join(".maestro").join("worktrees").join("x");
    std::fs::create_dir_all(&x).expect("mkdir x");
    let mut app = crate::tui::make_test_app("orphan-scan-1563");
    let mut stderr = Vec::new();
    let now = Instant::now();

    report_startup_orphans(&mut app, tmp.path(), now, Some(&mut stderr));

    let shown = visible(&app, now).unwrap_or_default();
    assert!(
        shown.starts_with("Error orphan worktree scan failed: git worktree list failed"),
        "scan error shown: {shown}"
    );
    let stderr = String::from_utf8(stderr).expect("utf-8");
    assert!(
        stderr.starts_with("warning: orphan worktree scan failed"),
        "{stderr}"
    );
    assert!(x.exists(), "a failed scan removes nothing");
}

/// A symlinked dir is shown on every launch and never gets a marker: the
/// marker would land outside the worktree root.
#[cfg(unix)]
#[test]
fn orphan_symlink_dir_gets_no_marker() {
    let (tmp, repo, leftover) = repo_with_leftover();
    std::fs::remove_dir_all(&leftover).expect("drop the plain dir");
    let target = tmp.path().join("elsewhere");
    std::fs::create_dir(&target).expect("mkdir target");
    std::os::unix::fs::symlink(&target, &leftover).expect("symlink");

    assert_eq!(orphan_notices(&repo), [LINE]);
    assert_eq!(orphan_notices(&repo), [LINE], "shown again, not recorded");
    assert!(!target.join(SHOWN_MARKER).exists(), "nothing written there");
}

#[test]
fn orphan_line_quotes_a_name_the_shell_would_split() {
    assert_eq!(
        orphan_line("it's mine"),
        r"untracked dir kept · rm -rf '.maestro/worktrees/it'\''s mine'"
    );
}
