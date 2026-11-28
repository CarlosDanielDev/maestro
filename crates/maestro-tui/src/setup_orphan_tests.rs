//! #1563: startup reports an untracked dir under `.maestro/worktrees/`
//! and never deletes it. Real git repos in a tempdir; every call passes
//! the repo root, so nothing depends on the process cwd.

use super::*;
use std::time::Instant;

/// `<tmp>/repo`, a git repo, with untracked `.maestro/worktrees/<name>`
/// dirs that each hold one file.
pub(crate) fn repo_with(names: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).expect("mkdir repo");
    let out = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .output()
        .expect("git must be on PATH");
    assert!(out.status.success(), "git init failed: {out:?}");
    for name in names {
        let dir = worktree(&repo, name);
        std::fs::create_dir_all(&dir).expect("mkdir orphan");
        std::fs::write(dir.join("file"), "only copy").expect("write file");
    }
    (tmp, repo)
}

pub(crate) fn worktree(repo: &Path, name: &str) -> PathBuf {
    repo.join(".maestro").join("worktrees").join(name)
}

const ONE: &str = "Error untracked dir kept: .maestro/worktrees/leftover";

fn toast(app: &App, now: Instant) -> Option<String> {
    (app.toasts.visible(now)).map(|v| format!("{:?} {}", v.severity, v.message))
}

/// One launch: a fresh App, the startup report, no stderr.
fn launch(repo: &Path) -> (App, Instant) {
    let mut app = crate::tui::make_test_app("orphan-1563");
    let now = Instant::now();
    report_startup_orphans(&mut app, repo, now, None);
    (app, now)
}

/// `Esc` on the visible toast (the key route is tested on Landing in
/// `snapshot_tests/startup_orphans.rs`).
fn esc(app: &mut App, now: Instant) {
    assert!(app.toast_esc(now), "a toast to dismiss");
}

fn marked(repo: &Path, names: &[&str]) -> Vec<bool> {
    (names.iter())
        .map(|n| worktree(repo, n).join(SHOWN_MARKER).exists())
        .collect()
}

#[test]
fn startup_orphan_untracked_dir_is_reported_not_deleted() {
    let (_tmp, repo) = repo_with(&["leftover"]);
    let mut app = crate::tui::make_test_app("orphan-1563");
    let mut stderr = Vec::new();
    let now = Instant::now();

    report_startup_orphans(&mut app, &repo, now, Some(&mut stderr));

    let dir = worktree(&repo, "leftover");
    assert!(dir.join("file").exists(), "the dir and its file stay");
    assert_eq!(toast(&app, now).as_deref(), Some(ONE));
    let stderr = String::from_utf8(stderr).expect("utf-8");
    assert!(
        stderr.ends_with("\nwarning: rm -rf .maestro/worktrees/leftover\n"),
        "--once prints the exact command: {stderr}"
    );
    assert!(dir.join(SHOWN_MARKER).exists(), "printed counts as shown");
}

/// Many dirs, or one long name: ONE toast in the count form that says
/// where to look; every exact command goes to the log (and stderr).
#[test]
fn startup_orphans_are_one_notice_with_every_command_logged() {
    let long = "unified-1563-1564-fix-core-startup-orphan-cleanup-never-re-1a2b";
    let names: Vec<String> = (1..=12).map(|i| format!("{long}-{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_tmp, repo) = repo_with(&names);
    let mut app = crate::tui::make_test_app("orphan-1563");
    let mut stderr = Vec::new();
    let now = Instant::now();

    report_startup_orphans(&mut app, &repo, now, Some(&mut stderr));

    assert_eq!(
        toast(&app, now).as_deref(),
        Some("Error 12 untracked dirs kept · check: ls .maestro/worktrees")
    );
    let stderr = String::from_utf8(stderr).expect("utf-8");
    for name in &names {
        let command = format!("warning: rm -rf .maestro/worktrees/{name}\n");
        assert!(stderr.contains(&command), "{command}");
    }

    let (_tmp, repo) = repo_with(&[long]);
    let (app, now) = launch(&repo);
    assert_eq!(
        toast(&app, now).as_deref(),
        Some("Error 1 untracked dir kept · check: ls .maestro/worktrees"),
        "a name too long to fit gets the count form"
    );
}

#[test]
fn orphan_notice_shown_once_and_again_after_recreate() {
    let names = ["leftover", "other"];
    let (_tmp, repo) = repo_with(&names);
    let two = "Error 2 untracked dirs kept · check: ls .maestro/worktrees";
    let (app, now) = launch(&repo);
    assert_eq!(toast(&app, now).as_deref(), Some(two));
    drop(app); // quit without dismissing it: not seen
    assert_eq!(marked(&repo, &names), [false, false], "nothing recorded");

    let (mut app, now) = launch(&repo);
    assert_eq!(toast(&app, now).as_deref(), Some(two), "shown again");
    esc(&mut app, now);
    assert!(toast(&app, now).is_none(), "Esc dismissed it");
    assert_eq!(marked(&repo, &names), [true, true], "Esc records all");

    let (app, now) = launch(&repo);
    assert!(toast(&app, now).is_none(), "the shown record stops it");

    let dir = worktree(&repo, "leftover");
    std::fs::remove_dir_all(&dir).expect("removed by hand");
    std::fs::create_dir(&dir).expect("recreated, same name");
    let (app, now) = launch(&repo);
    assert_eq!(toast(&app, now).as_deref(), Some(ONE), "a new dir");
}

/// Not a git repo: `git worktree list` fails (#1559). The error is shown
/// and printed, never dropped, and nothing is removed.
#[test]
fn startup_scan_error_is_reported() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let x = worktree(tmp.path(), "x");
    std::fs::create_dir_all(&x).expect("mkdir x");
    let mut app = crate::tui::make_test_app("orphan-scan-1563");
    let mut stderr = Vec::new();
    let now = Instant::now();

    report_startup_orphans(&mut app, tmp.path(), now, Some(&mut stderr));

    assert_eq!(
        toast(&app, now).as_deref(),
        Some("Error orphan worktree scan failed · see maestro.log")
    );
    let why = "orphan worktree scan failed: git worktree list failed";
    let stderr = String::from_utf8(stderr).expect("utf-8");
    assert!(stderr.starts_with(&format!("warning: {why}")), "{stderr}");
    assert!(x.exists(), "a failed scan removes nothing");
}

/// A symlinked dir never gets a marker (it would land outside the worktree
/// root), so it is shown on every launch, even after Esc.
#[cfg(unix)]
#[test]
fn orphan_symlink_dir_gets_no_marker() {
    let (tmp, repo) = repo_with(&[]);
    let target = tmp.path().join("elsewhere");
    std::fs::create_dir(&target).expect("mkdir target");
    let link = worktree(&repo, "leftover");
    std::fs::create_dir_all(link.parent().expect("parent")).expect("mkdir root");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    let (mut app, now) = launch(&repo);
    assert_eq!(toast(&app, now).as_deref(), Some(ONE));
    esc(&mut app, now);
    assert!(!target.join(SHOWN_MARKER).exists(), "nothing written there");
    let (app, now) = launch(&repo);
    assert_eq!(toast(&app, now).as_deref(), Some(ONE), "not recorded");
}

#[test]
fn remove_command_quotes_a_name_the_shell_would_split() {
    assert_eq!(
        remove_command("it's mine"),
        r"rm -rf '.maestro/worktrees/it'\''s mine'"
    );
}
