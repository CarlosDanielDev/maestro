//! Tests for `App::on_issue_session_completed` — extracted from the impl
//! file to keep `issue_completion.rs` under the 400-LOC budget.

#![cfg(test)]

use super::App;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::github::types::GhIssue;
use crate::session::worktree::MockWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::activity_log::LogLevel;

fn make_issue(number: u64) -> GhIssue {
    GhIssue {
        number,
        title: format!("Test issue #{}", number),
        body: String::new(),
        labels: vec![],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: None,
        assignees: vec![],
    }
}

fn make_test_config() -> crate::config::Config {
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
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

fn make_app_with_mock(mock: MockGitHubClient) -> App {
    let tmp = std::env::temp_dir().join(format!(
        "maestro-issue-completion-test-{}.json",
        uuid::Uuid::new_v4()
    ));
    let store = StateStore::new(tmp);
    let mut app = App::new(
        store,
        3,
        Box::new(MockWorktreeManager::new()),
        "bypassPermissions".into(),
        vec![],
    );
    app.gh_auth_ok = true;
    app.config = Some(make_test_config());
    app.github_client = Some(Box::new(mock));
    app
}

#[tokio::test]
async fn auto_pr_falls_back_to_github_fetch_when_cache_misses() {
    let mock = MockGitHubClient::new();
    mock.set_issues(vec![make_issue(42)]);
    mock.set_create_pr_response(101);
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    assert!(
        app.state.issue_cache.is_empty(),
        "precondition: issue_cache must be empty"
    );

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        1.23,
        vec!["src/foo.rs".into()],
        Some("maestro/issue-42".into()),
        false,
    )
    .await;

    let calls = mock_handle.create_pr_calls();
    assert_eq!(
        calls.len(),
        1,
        "PR must be created via the GitHub fallback path when the cache misses"
    );
    assert_eq!(calls[0].issue_number, 42);
}

#[tokio::test]
async fn auto_pr_logs_loudly_when_cache_miss_and_github_fetch_fails() {
    let mock = MockGitHubClient::new();
    mock.set_get_issue_error(42, "rate limited");
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    assert!(app.state.issue_cache.is_empty());

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        1.0,
        vec![],
        Some("maestro/issue-42".into()),
        false,
    )
    .await;

    assert!(
        mock_handle.create_pr_calls().is_empty(),
        "no PR call when both cache and GitHub cannot resolve the issue"
    );
    let logged_error = app.activity_log.entries().iter().any(|e| {
        matches!(e.level, LogLevel::Error) && e.message.contains("PR") && e.session_label == "#42"
    });
    assert!(
        logged_error,
        "activity log must record an Error entry naming the failed issue"
    );
}

#[tokio::test]
async fn auto_pr_uses_cached_issue_without_round_tripping_to_github() {
    // Mock has no seeded issues; if get_issue is called it would error.
    let mock = MockGitHubClient::new();
    mock.set_create_pr_response(202);
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.0,
        vec![],
        Some("maestro/issue-42".into()),
        false,
    )
    .await;

    assert_eq!(
        mock_handle.create_pr_calls().len(),
        1,
        "cached path must still create the PR (regression guard)"
    );
}
