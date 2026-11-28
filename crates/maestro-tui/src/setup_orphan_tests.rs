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

const ONE: &str = "kept 1 untracked dir · ⌃L shows how to remove";

fn toast(app: &App, now: Instant) -> Option<String> {
    (app.toasts.visible(now)).map(|v| format!("{:?} {}", v.severity, v.message))
}

fn logged(app: &App) -> Vec<String> {
    (app.activity_log.entries().iter())
        .map(|e| e.message.clone())
        .collect()
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

#[test]
fn startup_orphan_untracked_dir_is_reported_not_deleted() {
    let (_tmp, repo) = repo_with(&["leftover"]);
    let mut app = crate::tui::make_test_app("orphan-1563");
    let mut stderr = Vec::new();
    let now = Instant::now();

    report_startup_orphans(&mut app, &repo, now, Some(&mut stderr));

    let dir = worktree(&repo, "leftover");
    assert!(dir.join("file").exists(), "the dir and its file stay");
    assert_eq!(toast(&app, now), Some(format!("Error {ONE}")));
    let command = "rm -rf .maestro/worktrees/leftover";
    assert!(
        logged(&app).iter().any(|m| m == command),
        "{:?}",
        logged(&app)
    );
    let stderr = String::from_utf8(stderr).expect("utf-8");
    assert!(
        stderr.ends_with(&format!("\nwarning: {command}\n")),
        "--once prints the command: {stderr}"
    );
    assert!(dir.join(SHOWN_MARKER).exists(), "printed counts as shown");
}

/// Six dirs give ONE toast; every remove line is in the log.
#[test]
fn startup_orphans_are_one_notice() {
    let names = ["a1", "a2", "a3", "a4", "a5", "session-1a2b3c4d"];
    let (_tmp, repo) = repo_with(&names);
    let (app, now) = launch(&repo);

    assert_eq!(
        toast(&app, now).as_deref(),
        Some("Error kept 6 untracked dirs · ⌃L shows how to remove")
    );
    for name in names {
        let command = format!("rm -rf .maestro/worktrees/{name}");
        assert!(logged(&app).contains(&command), "{command}");
    }
}

#[test]
fn orphan_notice_shown_once_and_again_after_recreate() {
    let (_tmp, repo) = repo_with(&["leftover"]);
    let (app, now) = launch(&repo);
    assert_eq!(toast(&app, now), Some(format!("Error {ONE}")));
    drop(app); // quit without dismissing it: not seen

    let (mut app, now) = launch(&repo);
    assert_eq!(
        toast(&app, now),
        Some(format!("Error {ONE}")),
        "shown again"
    );
    esc(&mut app, now);
    assert!(toast(&app, now).is_none(), "Esc dismissed it");

    let (app, now) = launch(&repo);
    assert!(toast(&app, now).is_none(), "the shown record stops it");

    let dir = worktree(&repo, "leftover");
    std::fs::remove_dir_all(&dir).expect("removed by hand");
    std::fs::create_dir(&dir).expect("recreated, same name");
    let (app, now) = launch(&repo);
    assert_eq!(toast(&app, now), Some(format!("Error {ONE}")), "a new dir");
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
        Some("Error orphan worktree scan failed · ⌃L shows why")
    );
    let why = "orphan worktree scan failed: git worktree list failed";
    assert!(logged(&app).iter().any(|m| m.starts_with(why)));
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
    assert_eq!(toast(&app, now), Some(format!("Error {ONE}")));
    esc(&mut app, now);
    assert!(!target.join(SHOWN_MARKER).exists(), "nothing written there");
    let (app, now) = launch(&repo);
    assert_eq!(
        toast(&app, now),
        Some(format!("Error {ONE}")),
        "not recorded"
    );
}

#[test]
fn remove_command_quotes_a_name_the_shell_would_split() {
    assert_eq!(
        remove_command("it's mine"),
        [r"rm -rf '.maestro/worktrees/it'\''s mine'"]
    );
}

/// A long name is cut into rows that each fit the 80-column drawer with
/// their `\`; joined they are the one exact command.
#[test]
fn remove_command_of_a_long_name_fits_rows_and_joins_exactly() {
    let name = "unified-1563-1564-fix-core-startup-orphan-cleanup-never-re-1a2b";
    let pieces = remove_command(name);
    assert!(pieces.len() > 1, "{pieces:?}");
    assert!(pieces.iter().all(|p| p.chars().count() < LOG_ROW_CELLS));
    let command = format!("rm -rf .maestro/worktrees/{name}");
    assert_eq!(pieces.concat(), command);
}

/// `sh` reads the drawer rows (each but the last ends with `\`) as ONE
/// argument: the exact path, for plain and quoted names alike.
#[cfg(unix)]
#[test]
fn remove_command_rows_are_one_shell_word() {
    let long = "unified-1563-1564-fix-core-startup-orphan-cleanup-never-re-1a2b";
    for name in [long.to_string(), format!("{long} it's mine")] {
        let pieces = remove_command(&name);
        assert!(pieces.iter().all(|p| p.chars().count() < LOG_ROW_CELLS));
        let rows = pieces.join("\\\n");
        let word = rows.strip_prefix("rm -rf ").expect("command");
        let out = std::process::Command::new("sh")
            .args(["-c", &format!("printf '%s|' {word}")])
            .output()
            .expect("sh");
        let got = String::from_utf8(out.stdout).expect("utf-8");
        assert_eq!(got, format!(".maestro/worktrees/{name}|"), "{rows}");
    }
}
