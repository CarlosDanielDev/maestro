//! #1456: the kill dialog says what Kill really does to the worktree. The
//! session runs in a real git worktree, so the test reads the dialog, kills
//! through the real keys and checks the teardown against what it said.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tempfile::TempDir;

use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;

fn git(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be on PATH")
        .status
        .success()
}

/// A git repo with one commit on `main`.
fn repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "ci@example.com"],
        &["config", "user.name", "CI"],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        assert!(git(repo.path(), args), "git {args:?}");
    }
    repo
}

/// The Overview with one running session for `issue` in a real worktree.
fn running_in(repo: &Path, issue: u64) -> (App, uuid::Uuid, PathBuf) {
    let store = StateStore::new(repo.join("state-1456.json"));
    let worktrees = Box::new(GitWorktreeManager::new(repo.to_path_buf()));
    let mut app = App::new(store, 3, worktrees, "bypassPermissions".into(), vec![]);
    app.tui_mode = TuiMode::Dashboard;
    let session = Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    let id = session.id;
    app.pool.enqueue(session);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).unwrap();
    let worktree = managed.worktree_path.clone().expect("a real worktree");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    (app, id, worktree)
}

async fn press(app: &mut App, code: KeyCode) {
    let _ = handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

#[tokio::test]
async fn kill_dialog_tells_the_truth_about_worktrees() {
    let repo = repo();
    let (mut app, id, worktree) = running_in(repo.path(), 1456);
    std::fs::write(worktree.join("work.txt"), "uncommitted").unwrap();

    press(&mut app, KeyCode::Char('X')).await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    let out = draw(&mut app);
    assert!(
        out.contains("gone   the worktree and branch maestro/issue-1456"),
        "the dialog names what Kill deletes:\n{out}"
    );
    assert!(
        out.contains("lost   1 uncommitted file, the turn in progress"),
        "the uncommitted file is listed as lost:\n{out}"
    );
    assert!(
        !out.contains("kept   the worktree"),
        "the worktree is never promised as kept:\n{out}"
    );

    for c in "1456".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    press(&mut app, KeyCode::Enter).await;
    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Killed));
    app.check_completions().await.unwrap();

    // What the dialog said is what happened.
    assert!(!worktree.exists(), "the worktree is deleted");
    assert!(
        !git(
            repo.path(),
            &["rev-parse", "--verify", "-q", "maestro/issue-1456"]
        ),
        "the local branch, with no commit of its own, is deleted"
    );
}

/// A clean worktree has nothing uncommitted to lose: no count is shown.
#[tokio::test]
async fn kill_dialog_on_a_clean_worktree_counts_no_files() {
    let repo = repo();
    let (mut app, _id, _worktree) = running_in(repo.path(), 1457);
    press(&mut app, KeyCode::Char('X')).await;
    let out = draw(&mut app);
    assert!(out.contains("lost   the turn in progress"), "{out}");
    assert!(!out.contains("uncommitted"), "{out}");
    assert!(out.contains("gone   the worktree"), "{out}");
}

/// Git cannot read the worktree (the path is gone): the dialog still warns,
/// with the count unknown, and never says nothing is lost.
#[tokio::test]
async fn kill_dialog_with_an_unreadable_worktree_says_unknown() {
    let repo = repo();
    let (mut app, id, _worktree) = running_in(repo.path(), 1458);
    let managed = app.pool.get_active_mut(id).unwrap();
    // The teardown path shape, but gone from disk: git cannot read it.
    let gone = repo.path().join("moved/.maestro/worktrees/issue-1458");
    managed.worktree_path = Some(gone);
    press(&mut app, KeyCode::Char('X')).await;
    let out = draw(&mut app);
    assert!(
        out.contains("lost   uncommitted files (count unknown), turn in progress"),
        "{out}"
    );
    assert!(out.contains("gone   the worktree"), "{out}");
    // #1469: git cannot count the unpushed commits either, and teardown
    // keeps a branch it cannot count: never "the branch goes".
    assert!(!out.contains("and branch"), "{out}");
    assert!(
        out.contains("kept   branch maestro/issue-1458 if it has unpushed commits"),
        "{out}"
    );
}

/// #1469: a session that committed but never pushed. The dialog says the
/// worktree goes and the branch is kept, and the real teardown agrees.
#[tokio::test]
async fn kill_dialog_keeps_a_branch_with_unpushed_commits() {
    let repo = repo();
    let (mut app, _id, worktree) = running_in(repo.path(), 1461);
    assert!(git(
        &worktree,
        &["commit", "-q", "--allow-empty", "-m", "work"]
    ));

    press(&mut app, KeyCode::Char('X')).await;
    let out = draw(&mut app);
    assert!(
        !out.contains("and branch"),
        "the branch is not deleted:\n{out}"
    );
    assert!(out.contains("gone   the worktree"), "{out}");
    assert!(
        out.contains("kept   branch maestro/issue-1461, 1 unpushed commit"),
        "the dialog says the branch and its commit stay:\n{out}"
    );

    for c in "1461".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    press(&mut app, KeyCode::Enter).await;
    app.check_completions().await.unwrap();

    assert!(!worktree.exists(), "the worktree is deleted");
    assert!(
        git(
            repo.path(),
            &["rev-parse", "--verify", "-q", "maestro/issue-1461"]
        ),
        "the branch with the unpushed commit is kept"
    );
}

/// Judge (#1468): an interaction whose worktree failed runs in `.`, the
/// main checkout. Teardown removes `.maestro/worktrees/<slug>` only, so the
/// dialog must not say a worktree goes, nor count the checkout's files.
#[tokio::test]
async fn kill_dialog_in_the_main_checkout_promises_no_worktree_loss() {
    let repo = repo();
    let (mut app, id, _worktree) = running_in(repo.path(), 1459);
    app.pool.get_active_mut(id).unwrap().worktree_path = Some(PathBuf::from("."));
    press(&mut app, KeyCode::Char('X')).await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    let out = draw(&mut app);
    assert!(!out.contains("gone "), "no worktree goes:\n{out}");
    assert!(
        !out.contains("uncommitted"),
        "the checkout loses nothing:\n{out}"
    );
    assert!(out.contains("lost   the turn in progress"), "{out}");
}

/// Judge (#1468): `Errored` has no `Killed` transition, so Kill would do
/// nothing and teardown keeps the worktree. `X` says so and opens no
/// dialog that promises a deletion.
#[tokio::test]
async fn kill_on_an_errored_session_says_nothing_changes() {
    let repo = repo();
    let (mut app, id, worktree) = running_in(repo.path(), 1460);
    let managed = app.pool.get_active_mut(id).unwrap();
    managed
        .session
        .transition_to(SessionStatus::Errored, TransitionReason::StreamError)
        .unwrap();
    press(&mut app, KeyCode::Char('X')).await;
    assert!(
        !matches!(app.tui_mode, TuiMode::ConfirmKill(_)),
        "no kill dialog for a session Kill cannot end"
    );
    let out = draw(&mut app);
    assert!(out.contains("Kill cannot stop it"), "{out}");

    app.check_completions().await.unwrap();
    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Errored));
    assert!(worktree.exists(), "teardown keeps an errored worktree");
}
