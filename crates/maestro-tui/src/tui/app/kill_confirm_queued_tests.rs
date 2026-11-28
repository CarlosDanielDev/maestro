//! #1556: the dialog for a queued retry child says what removing it does
//! to the worktree its parent left it, and `⏎` does exactly that.

use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;

use super::{draw, git, press, repo};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::{App, TuiMode};

/// The Overview over real worktrees of `repo`, with `max` session slots.
fn app_in(repo: &Path, max: usize) -> App {
    let store = StateStore::new(repo.join("state-1556.json"));
    let worktrees = Box::new(GitWorktreeManager::new(repo.to_path_buf()));
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
