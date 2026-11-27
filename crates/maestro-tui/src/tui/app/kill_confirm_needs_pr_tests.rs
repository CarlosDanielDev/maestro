//! #1500: a session waiting for its PR (NeedsPr) has no agent process, but
//! Kill can still end it. The dialog says what is really there, and the
//! real teardown matches it. Kill cancels the issue's PR retry, so no PR,
//! CI poll or CI fix agent follows for a killed session.

use std::sync::Arc;

use super::*;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::github::types::{PendingPrStatus, awaiting_pending_pr};
use crate::provider::types::Issue;

/// A NeedsPr session for `issue` with a PR retry in `status`, a fake gh,
/// and the issue cached (the retry needs it to draft the PR).
fn waiting_for_pr(
    repo: &Path,
    issue: u64,
    status: PendingPrStatus,
) -> (App, uuid::Uuid, PathBuf, Arc<MockGitHubClient>) {
    let (mut app, id, worktree) = running_in(repo, issue);
    let managed = app.pool.get_active_mut(id).unwrap();
    managed
        .session
        .transition_to(SessionStatus::NeedsPr, TransitionReason::PrNeeded)
        .unwrap();
    let mut pending = awaiting_pending_pr(issue);
    pending.status = status;
    pending.next_retry_at = Some(chrono::Utc::now());
    app.pending_prs.push(pending);
    let gh = Arc::new(MockGitHubClient::new());
    app.github_client = Some(gh.clone());
    app.state.issue_cache.insert(
        issue,
        Issue {
            number: issue,
            title: format!("Issue #{issue}"),
            body: String::new(),
            labels: vec![],
            state: "open".into(),
            html_url: format!("https://github.com/owner/repo/issues/{issue}"),
            milestone: None,
            assignees: vec![],
        },
    );
    (app, id, worktree, gh)
}

async fn kill(app: &mut App, issue: u64) {
    for c in issue.to_string().chars() {
        press(app, KeyCode::Char(c)).await;
    }
    press(app, KeyCode::Enter).await;
}

/// After the kill, the retry tick (and a manual retry) opens no PR, adds
/// no CI check and queues no agent.
async fn assert_nothing_follows(app: &mut App, issue: u64, gh: &MockGitHubClient) {
    assert!(app.pending_prs.is_empty(), "the PR retry is cancelled");
    app.trigger_manual_pr_retry(issue);
    app.process_pending_pr_retries().await;
    assert!(gh.create_pr_calls().is_empty(), "no PR is opened");
    assert!(app.ci_poller.pending_pr_checks.is_empty(), "no CI poll");
    assert!(app.pending_session_launches.is_empty(), "no new agent");
}

#[tokio::test]
async fn needs_pr_session_can_be_killed() {
    let repo = repo();
    let (mut app, id, worktree, gh) =
        waiting_for_pr(repo.path(), 1500, PendingPrStatus::RetryScheduled);

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
        out.contains("gone   its PR retry: no PR is opened"),
        "the dialog says the PR retry goes:\n{out}"
    );

    kill(&mut app, 1500).await;
    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Killed));
    assert_nothing_follows(&mut app, 1500, &gh).await;

    app.check_completions().await.unwrap();
    assert!(!worktree.exists(), "the worktree goes, as the dialog said");
    assert!(
        !git(
            repo.path(),
            &["rev-parse", "--verify", "-q", "maestro/issue-1500"]
        ),
        "the branch goes, as the dialog said"
    );
}

/// A retry waiting for the manual key is cancelled too.
#[tokio::test]
async fn kill_cancels_a_manual_wait_pr_retry() {
    let repo = repo();
    let (mut app, _id, _worktree, gh) =
        waiting_for_pr(repo.path(), 1501, PendingPrStatus::AwaitingManualRetry);

    press(&mut app, KeyCode::Char('X')).await;
    let out = draw(&mut app);
    assert!(
        out.contains("gone   its PR retry: no PR is opened"),
        "{out}"
    );

    kill(&mut app, 1501).await;
    assert_nothing_follows(&mut app, 1501, &gh).await;
}
