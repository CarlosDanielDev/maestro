//! #1556: the dialog for a queued retry child says what removing it does
//! to the worktree its parent left it, and `⏎` does exactly that.

use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;

use super::{draw, git, press, repo};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::{GitWorktreeManager, WorktreeManager};
use crate::state::store::StateStore;
use crate::tui::app::{App, TuiMode};

/// The Overview over real worktrees of `repo`, with `max` session slots.
fn app_in(repo: &Path, max: usize) -> App {
    app_over(
        repo,
        max,
        Box::new(GitWorktreeManager::new(repo.to_path_buf())),
    )
}

fn app_over(repo: &Path, max: usize, worktrees: Box<dyn WorktreeManager + Send>) -> App {
    let store = StateStore::new(repo.join("state-1556.json"));
    let mut app = App::new(store, max, worktrees, "bypassPermissions".into(), vec![]);
    app.tui_mode = TuiMode::Dashboard;
    app
}

fn for_issue(issue: u64) -> Session {
    let (prompt, model, mode) = ("probe".into(), "opus".into(), "orchestrator".into());
    Session::new(prompt, model, mode, Some(issue), None)
}

/// Promote a session for `issue` and run it; returns its id and worktree.
fn running(app: &mut App, issue: u64) -> (uuid::Uuid, PathBuf) {
    let session = for_issue(issue);
    let id = session.id;
    app.pool.enqueue(session);
    assert_eq!(app.pool.try_promote(), vec![id]);
    let managed = app.pool.get_active_mut(id).unwrap();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    (id, managed.worktree_path.clone().expect("a real worktree"))
}

/// The session stalls and its retry is triggered.
fn retrying(app: &mut App, id: uuid::Uuid) {
    let managed = app.pool.get_active_mut(id).unwrap();
    for (to, why) in [
        (SessionStatus::Stalled, TransitionReason::HealthStall),
        (SessionStatus::Retrying, TransitionReason::RetryTriggered),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
}

/// Queue a retry child for `issue`; returns its id.
fn queue_child(app: &mut App, issue: u64) -> uuid::Uuid {
    let child = for_issue(issue);
    let id = child.id;
    app.pool.enqueue(child);
    id
}

/// Select `id` on the Overview, open its dialog and return the frame.
async fn open_dialog(app: &mut App, id: uuid::Uuid) -> String {
    let index = (app.pool.all_sessions().iter()).position(|s| s.id == id);
    app.mark_session_at(index.expect("listed"));
    press(app, KeyCode::Char('X')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmKill(id));
    draw(app)
}

async fn confirm(app: &mut App, token: &str) {
    for c in token.chars() {
        press(app, KeyCode::Char(c)).await;
    }
    press(app, KeyCode::Enter).await;
}

#[tokio::test]
async fn kill_on_queued_retry_child_says_its_handed_worktree_goes() {
    let repo = repo();
    let mut app = app_in(repo.path(), 1);
    let (parent, worktree) = running(&mut app, 1556);
    std::fs::write(worktree.join("work.txt"), "uncommitted").unwrap();
    retrying(&mut app, parent);
    let child = queue_child(&mut app, 1556);
    assert!(
        app.pool.finalize(parent),
        "the parent hands its worktree over"
    );

    let out = open_dialog(&mut app, child).await;
    assert!(
        out.contains("gone   its place in the queue: it never starts"),
        "the dialog says the queue place goes:\n{out}"
    );
    assert!(
        out.contains("gone   the worktree and its branch"),
        "the dialog says the handed worktree goes:\n{out}"
    );
    assert!(
        out.contains("lost   1 uncommitted file"),
        "the file left in it is listed as lost:\n{out}"
    );
    assert!(
        !out.contains("no worktree is touched"),
        "the worktree is never promised as untouched:\n{out}"
    );

    confirm(&mut app, "1556").await;
    let status = app.pool.get_session(child).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Killed));
    // What the dialog said is what happened.
    assert!(!worktree.exists(), "the worktree is deleted");
    let branch = ["rev-parse", "--verify", "-q", "maestro/issue-1556"];
    assert!(
        !git(repo.path(), &branch),
        "the branch, with nothing of its own, is deleted"
    );
}

#[tokio::test]
async fn kill_on_queued_child_of_running_parent_says_no_worktree_is_touched() {
    let repo = repo();
    let mut app = app_in(repo.path(), 2);
    let (first, worktree) = running(&mut app, 1557);
    retrying(&mut app, first);
    // The first retry runs in the shared worktree; its own retry is queued.
    let (parent, _) = running(&mut app, 1557);
    let child = queue_child(&mut app, 1557);
    assert!(app.pool.finalize(first), "the first session is finalized");

    let out = open_dialog(&mut app, child).await;
    assert!(
        out.contains("kept   every file: no worktree is touched"),
        "the parent still runs there, so nothing is removed:\n{out}"
    );
    assert!(
        !out.contains("gone   the worktree") && !out.contains("lost "),
        "the dialog never says the worktree goes:\n{out}"
    );

    confirm(&mut app, "1557").await;
    let status = app.pool.get_session(child).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Killed));
    assert!(worktree.exists(), "the parent's worktree stays");
    assert!(
        app.pool.get_active(parent).is_some(),
        "the parent still runs"
    );
}

/// The toast on screen now, if any.
fn toast(app: &App) -> Option<String> {
    let now = std::time::Instant::now();
    app.toasts.visible(now).map(|t| t.message.to_string())
}

/// #1556 judge: the dialog opened before the parent handed its worktree
/// over and said nothing is touched. `⏎` keeps that promise, and says so.
#[tokio::test]
async fn kill_on_queued_child_keeps_the_promise_when_the_parent_hands_over_first() {
    let repo = repo();
    let mut app = app_in(repo.path(), 1);
    let (parent, worktree) = running(&mut app, 1561);
    std::fs::write(worktree.join("work.txt"), "uncommitted").unwrap();
    retrying(&mut app, parent);
    let child = queue_child(&mut app, 1561);

    let out = open_dialog(&mut app, child).await;
    assert!(
        out.contains("kept   every file: no worktree is touched"),
        "the parent still runs when the dialog opens:\n{out}"
    );
    assert!(
        app.pool.finalize(parent),
        "the parent hands its worktree over"
    );
    confirm(&mut app, "1561").await;

    let status = app.pool.get_session(child).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Killed));
    assert!(worktree.join("work.txt").exists(), "no file is lost");
    let message = toast(&app).unwrap_or_default();
    assert!(
        message.contains("its worktree was kept, as the dialog said"),
        "the user is told the worktree stays: {message:?}"
    );
}

/// Git that never removes the worktree and says nothing, like
/// `GitWorktreeManager::remove` when git fails or refuses.
struct StuckWorktrees(GitWorktreeManager);

impl WorktreeManager for StuckWorktrees {
    fn create(&self, slug: &str) -> anyhow::Result<PathBuf> {
        self.0.create(slug)
    }
    fn remove(&self, _slug: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn exists(&self, slug: &str) -> bool {
        self.0.exists(slug)
    }
}

#[tokio::test]
async fn kill_on_queued_retry_child_says_when_git_kept_the_worktree() {
    let repo = repo();
    let stuck = StuckWorktrees(GitWorktreeManager::new(repo.path().to_path_buf()));
    let mut app = app_over(repo.path(), 1, Box::new(stuck));
    let (parent, worktree) = running(&mut app, 1562);
    retrying(&mut app, parent);
    let child = queue_child(&mut app, 1562);
    assert!(
        app.pool.finalize(parent),
        "the parent hands its worktree over"
    );

    let out = open_dialog(&mut app, child).await;
    assert!(out.contains("gone   the worktree"), "promised:\n{out}");
    confirm(&mut app, "1562").await;

    assert!(worktree.exists(), "git did not remove it");
    let message = toast(&app).unwrap_or_default();
    assert!(
        message.contains("git could not remove its worktree"),
        "the failed teardown is shown, not only logged: {message:?}"
    );
}

/// #1469 on this path: the branch with an unpushed commit is kept.
#[tokio::test]
async fn kill_on_queued_retry_child_keeps_a_branch_with_unpushed_commits() {
    let repo = repo();
    let mut app = app_in(repo.path(), 1);
    let (parent, worktree) = running(&mut app, 1563);
    assert!(git(
        &worktree,
        &["commit", "-q", "--allow-empty", "-m", "work"]
    ));
    retrying(&mut app, parent);
    let child = queue_child(&mut app, 1563);
    assert!(
        app.pool.finalize(parent),
        "the parent hands its worktree over"
    );

    let out = open_dialog(&mut app, child).await;
    assert!(
        out.contains("kept   its branch, 1 unpushed commit"),
        "the dialog says the branch stays:\n{out}"
    );
    confirm(&mut app, "1563").await;

    assert!(!worktree.exists(), "the worktree is deleted");
    let branch = ["rev-parse", "--verify", "-q", "maestro/issue-1563"];
    assert!(git(repo.path(), &branch), "the branch is kept");
}

/// A session resumed into the queue with its own worktree (#1266) was
/// handed nothing: the dialog and `⏎` leave its files alone.
#[tokio::test]
async fn kill_on_resumed_queued_session_never_touches_its_worktree() {
    let repo = repo();
    let mut app = app_in(repo.path(), 1);
    let (before, worktree) = running(&mut app, 1564);
    let managed = app.pool.get_active_mut(before).unwrap();
    let errored = (SessionStatus::Errored, TransitionReason::StreamError);
    managed.session.transition_to(errored.0, errored.1).unwrap();
    assert!(
        app.pool.finalize(before),
        "an errored session keeps its worktree"
    );
    let mut resumed = for_issue(1564);
    resumed.worktree_path = Some(worktree.clone());
    let id = resumed.id;
    app.pool.enqueue(resumed);

    let out = open_dialog(&mut app, id).await;
    assert!(
        out.contains("kept   every file: no worktree is touched"),
        "nothing was handed to it:\n{out}"
    );
    confirm(&mut app, "1564").await;

    assert!(worktree.exists(), "its worktree stays");
}
