//! #1241 — `gh_auth_ok` comes back without a restart: a successful gh
//! call flips it to true, and a PR action rechecks auth once before it
//! refuses. Declared in `completion_pipeline.rs` to reuse the
//! completion-job fixtures from `completion_off_loop_tests.rs`.

use super::completion_off_loop_tests::{
    drive_until_applied, enqueue_pending_completion, make_app_with,
};
use crate::git::MockGitOps;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::types::Issue;
use crate::tui::app::App;
use crate::tui::app::TuiDataEvent;
use crate::tui::app::gh_auth::{GhAuthProbe, recheck_gh_auth};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn auth_error() -> anyhow::Error {
    anyhow::anyhow!("[gh-auth-error] HTTP 401: token expired")
}

/// A probe that answers `signed_in` and counts how often it ran.
fn counting_probe(signed_in: bool) -> (GhAuthProbe, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let probe: GhAuthProbe = Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        signed_in
    });
    (probe, calls)
}

fn config_auto_pr_enabled() -> crate::config::Config {
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
[gates]
enabled = false
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
auto_pr = true
[notifications]
"#;
    toml::from_str(toml_str).expect("test config parse")
}

fn issue(number: u64) -> Issue {
    Issue {
        number,
        title: format!("Test issue #{number}"),
        body: String::new(),
        labels: vec![],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{number}"),
        milestone: None,
        assignees: vec![],
    }
}

/// App signed out of gh, with a PR-ready completion queued for #1241.
fn signed_out_app_with_completion(
    probe: GhAuthProbe,
    mock: Arc<MockGitHubClient>,
    worktree: &std::path::Path,
) -> App {
    let mut app = make_app_with(
        config_auto_pr_enabled(),
        MockGitOps::new().with_commits_ahead(true),
    );
    app.github_client = Some(mock);
    app.gh_auth_probe = probe;
    app.state.issue_cache.insert(1241, issue(1241));
    app.handle_data_event(TuiDataEvent::Issues(Err(auth_error())));
    assert!(
        !app.gh_auth_ok,
        "precondition: an auth error clears the flag"
    );
    enqueue_pending_completion(&mut app, 1241, "maestro/issue-1241", worktree.to_path_buf());
    app
}

#[test]
fn gh_auth_recovers_after_success() {
    let mut app = crate::tui::make_test_app("gh-auth-recovers");

    app.handle_data_event(TuiDataEvent::Issues(Err(auth_error())));
    assert!(!app.gh_auth_ok, "an auth error clears the flag");
    app.handle_data_event(TuiDataEvent::Issues(Ok(vec![])));
    assert!(app.gh_auth_ok, "a successful gh call sets the flag back");
    assert!(
        app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("GitHub authentication restored")),
        "recovery is visible in the activity log"
    );

    // A second loss recovers the same way — no restart, no one-shot latch.
    app.handle_data_event(TuiDataEvent::PullRequests(Err(auth_error())));
    assert!(!app.gh_auth_ok);
    app.handle_data_event(TuiDataEvent::PullRequests(Ok(vec![])));
    assert!(app.gh_auth_ok, "a second successful gh call clears it too");

    app.handle_data_event(TuiDataEvent::Milestones(Err(auth_error())));
    assert!(!app.gh_auth_ok);
    app.handle_data_event(TuiDataEvent::Milestones(Ok(Default::default())));
    assert!(app.gh_auth_ok, "milestone fetches count as gh calls too");

    app.handle_data_event(TuiDataEvent::PrReviewSubmitted(Err(auth_error())));
    assert!(!app.gh_auth_ok);
    app.handle_data_event(TuiDataEvent::PrReviewSubmitted(Ok(())));
    assert!(app.gh_auth_ok, "a submitted review is a gh call too");
}

#[test]
fn gh_auth_success_while_signed_in_logs_nothing() {
    let mut app = crate::tui::make_test_app("gh-auth-quiet");
    app.handle_data_event(TuiDataEvent::Issues(Ok(vec![])));
    assert!(app.gh_auth_ok);
    assert!(
        !app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("restored")),
        "no recovery line when auth was never lost"
    );
}

#[tokio::test]
async fn pr_action_rechecks_gh_auth() {
    let (probe, calls) = counting_probe(true);
    let mock = Arc::new(MockGitHubClient::new());
    mock.set_create_pr_response(77);
    let wt = tempfile::tempdir().unwrap();
    let mut app = signed_out_app_with_completion(probe, Arc::clone(&mock), wt.path());

    app.check_completions().await.unwrap();
    drive_until_applied(&mut app).await;

    assert_eq!(calls.load(Ordering::SeqCst), 1, "one recheck per PR action");
    assert_eq!(
        mock.create_pr_calls().len(),
        1,
        "signed in again: PR is created"
    );
    assert!(app.gh_auth_ok, "the recheck sets the flag back");
    assert!(
        app.pending_prs.iter().all(|p| p.issue_number != 1241),
        "nothing deferred to the Shift+P queue"
    );
}

#[tokio::test]
async fn pr_action_rechecks_gh_auth_then_refuses_when_still_signed_out() {
    let (probe, calls) = counting_probe(false);
    let mock = Arc::new(MockGitHubClient::new());
    mock.set_create_pr_response(77);
    let wt = tempfile::tempdir().unwrap();
    let mut app = signed_out_app_with_completion(probe, Arc::clone(&mock), wt.path());

    app.check_completions().await.unwrap();
    drive_until_applied(&mut app).await;

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "rechecked once before refusing"
    );
    assert!(mock.create_pr_calls().is_empty(), "still signed out: no PR");
    assert!(!app.gh_auth_ok);
    assert!(
        app.pending_prs.iter().any(|p| p.issue_number == 1241),
        "the refusal defers the PR to the Shift+P queue"
    );
}

#[tokio::test]
async fn pr_action_skips_recheck_while_signed_in() {
    let (probe, calls) = counting_probe(true);
    let mock = Arc::new(MockGitHubClient::new());
    mock.set_create_pr_response(77);
    let wt = tempfile::tempdir().unwrap();
    let mut app = signed_out_app_with_completion(probe, Arc::clone(&mock), wt.path());
    app.gh_auth_ok = true;

    app.check_completions().await.unwrap();
    drive_until_applied(&mut app).await;

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no subprocess while signed in"
    );
    assert_eq!(mock.create_pr_calls().len(), 1);
}

/// A probe stuck on the network must not hold the completion job: past
/// the timeout the recheck answers "signed out" and the PR is refused.
#[tokio::test]
async fn gh_auth_recheck_times_out_as_signed_out() {
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    let probe: GhAuthProbe = Arc::new(move || {
        let _ = wait.lock().unwrap().recv();
        true
    });

    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        recheck_gh_auth(&probe, std::time::Duration::from_millis(50)),
    )
    .await;
    drop(release); // let the parked probe thread finish

    assert_eq!(answer, Ok(false), "a stuck probe counts as signed out");
}
