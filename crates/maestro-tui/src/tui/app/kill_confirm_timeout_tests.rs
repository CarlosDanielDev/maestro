//! #1481: the Kill dialog opens even when git cannot count the branch's
//! unpushed commits in time. Sibling file of `kill_confirm_tests` (that
//! one is near the 400-line cap).

use super::*;

/// A FIFO no one writes to as the object-store alternates file makes
/// `git rev-list` block on `open` until it is killed: a real stuck git,
/// no sleep. The count gives up at the timeout, the dialog still opens,
/// and it never says the branch goes.
#[cfg(unix)]
#[tokio::test]
async fn kill_dialog_opens_when_unique_count_times_out() {
    let repo = repo();
    let (mut app, _id, _worktree) = running_in(repo.path(), 1481);
    let alternates = repo.path().join(".git/objects/info/alternates");
    std::fs::create_dir_all(alternates.parent().unwrap()).unwrap();
    let made = Command::new("mkfifo").arg(&alternates).status().unwrap();
    assert!(made.success(), "mkfifo");

    let started = Instant::now();
    press(&mut app, KeyCode::Char('X')).await;
    // The helper killed every stuck git; the FIFO goes before any assert
    // can fail, so nothing else can block on it.
    std::fs::remove_file(&alternates).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the dialog does not wait for git"
    );
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    let out = draw(&mut app);
    assert!(!out.contains("and branch"), "{out}");
    assert!(
        out.contains("kept   branch maestro/issue-1481 if it has unpushed commits"),
        "a count that timed out reads unknown, never nothing:\n{out}"
    );
}
