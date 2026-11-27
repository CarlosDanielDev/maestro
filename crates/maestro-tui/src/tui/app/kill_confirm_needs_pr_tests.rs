//! #1500: a session waiting for its PR (NeedsPr) has no agent process, but
//! Kill can still end it. The dialog says what is really there, and the
//! real teardown matches it.

use super::*;
use crate::provider::github::types::awaiting_pending_pr;

#[tokio::test]
async fn needs_pr_session_can_be_killed() {
    let repo = repo();
    let (mut app, id, worktree) = running_in(repo.path(), 1500);
    let managed = app.pool.get_active_mut(id).unwrap();
    managed
        .session
        .transition_to(SessionStatus::NeedsPr, TransitionReason::PrNeeded)
        .unwrap();
    app.pending_prs.push(awaiting_pending_pr(1500));

    press(&mut app, KeyCode::Char('X')).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::ConfirmKill(id),
        "Kill opens its dialog"
    );
    let out = draw(&mut app);
    assert!(
        out.contains("No agent runs: it is waiting for its PR."),
        "{out}"
    );
    assert!(!out.contains("Stops the agent"), "no agent to stop:\n{out}");
    assert!(!out.contains("turn in progress"), "no turn to lose:\n{out}");
    assert!(out.contains("lost   nothing uncommitted"), "{out}");
    assert!(
        out.contains("gone   the worktree and branch maestro/issue-1500"),
        "{out}"
    );
    assert!(
        out.contains("kept   its PR retry: the PR may still open"),
        "Kill leaves the PR retry alone and says so:\n{out}"
    );

    for c in "1500".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    press(&mut app, KeyCode::Enter).await;
    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Killed));
    app.check_completions().await.unwrap();
    assert!(!worktree.exists(), "the worktree goes, as the dialog said");
}
