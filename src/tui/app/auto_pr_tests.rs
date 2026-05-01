//! Behavior tests for the auto-PR pipeline (#514). Lives next to
//! `auto_pr.rs` so the implementation file stays focused. Reuses
//! `issue_completion_tests.rs` test fixtures (`make_app_with_mock`,
//! `make_issue`, `make_test_config`) via direct re-import.

#![cfg(test)]

use super::App;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::github::types::{GhIssue, GhPullRequest};
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
        "maestro-auto-pr-test-{}.json",
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

fn make_pr(number: u64, head_branch: &str) -> GhPullRequest {
    GhPullRequest {
        number,
        title: format!("Existing PR #{}", number),
        body: String::new(),
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/pull/{}", number),
        head_branch: head_branch.to_string(),
        base_branch: "main".to_string(),
        author: "bot".to_string(),
        labels: vec![],
        draft: false,
        mergeable: true,
        additions: 0,
        deletions: 0,
        changed_files: 0,
    }
}

#[tokio::test]
async fn auto_pr_logs_url_in_activity_log_on_success() {
    let mock = MockGitHubClient::new();
    mock.set_create_pr_response(101);
    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        1.23,
        vec!["src/foo.rs".into()],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    let has_url_log = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("PR #101")
            && e.message.contains("https://github.com/owner/repo/pull/101")
            && matches!(e.level, LogLevel::Info)
    });
    assert!(
        has_url_log,
        "activity log must include the PR URL on success (AC1). Entries: {:?}",
        app.activity_log
            .entries()
            .iter()
            .map(|e| (e.session_label.clone(), e.message.clone()))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn auto_pr_skips_creation_when_pr_already_exists_for_branch() {
    let mock = MockGitHubClient::new();
    mock.set_create_pr_response(999); // would be visible if (wrongly) called
    mock.set_list_prs_for_branch("maestro/issue-42", vec![55]);
    mock.set_pull_requests(vec![make_pr(55, "maestro/issue-42")]);
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    assert!(
        mock_handle.create_pr_calls().is_empty(),
        "create_pr must not be called when a PR already exists for the branch (AC4)"
    );
    let has_existing_log = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("https://github.com/owner/repo/pull/55")
            && matches!(e.level, LogLevel::Info)
    });
    assert!(
        has_existing_log,
        "activity log must reference the existing PR URL (AC4)"
    );
}

#[tokio::test]
async fn auto_pr_does_not_double_fire_within_one_process() {
    let mock = MockGitHubClient::new();
    mock.set_create_pr_response(101);
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;
    // Second call simulates a duplicate event — must not create a second PR.
    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    assert_eq!(
        mock_handle.create_pr_calls().len(),
        1,
        "create_pr must fire exactly once per session-end, even on duplicate calls (AC7)"
    );
}

#[tokio::test]
async fn auto_pr_general_error_logs_branch_and_manual_command() {
    let mock = MockGitHubClient::new();
    mock.set_create_pr_error("network timeout");
    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    let has_manual_hint = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("maestro/issue-42")
            && e.message.contains("gh pr create")
            && e.message.contains("--head maestro/issue-42")
    });
    assert!(
        has_manual_hint,
        "error path must surface the branch name and a manual `gh pr create` command (AC6)"
    );
    assert_eq!(
        app.pending_prs.len(),
        1,
        "failure must still queue a retry in pending_prs"
    );
}

#[tokio::test]
async fn auto_pr_disabled_logs_visible_skip_message() {
    let mock = MockGitHubClient::new();
    let mock_handle = mock.clone();
    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));
    // Override config so auto_pr = false.
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
auto_pr = false
[notifications]
"#;
    app.config = Some(toml::from_str(toml_str).expect("test config parse"));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    assert!(
        mock_handle.create_pr_calls().is_empty(),
        "no PR call when auto_pr disabled"
    );
    let has_disabled_log = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("Auto-PR disabled")
            && matches!(e.level, LogLevel::Info)
    });
    assert!(
        has_disabled_log,
        "must log when auto-PR is disabled (closing the silent-skip gap)"
    );
}

#[tokio::test]
async fn auto_pr_no_github_client_logs_error_not_silent() {
    let mock = MockGitHubClient::new();
    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));
    app.github_client = None;

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    let has_no_client_log = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("GitHub client")
            && matches!(e.level, LogLevel::Error)
    });
    assert!(
        has_no_client_log,
        "must log error when github_client is None (closing the silent-skip gap)"
    );
}

#[tokio::test]
async fn auto_pr_no_worktree_branch_logs_error_not_silent() {
    let mock = MockGitHubClient::new();
    let mut app = make_app_with_mock(mock);
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        None, // no worktree branch
        None,
        false,
    )
    .await;

    let has_no_branch_log = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("no worktree branch")
            && matches!(e.level, LogLevel::Error)
    });
    assert!(
        has_no_branch_log,
        "must log error when worktree_branch is None (closing the silent-skip gap)"
    );
}

// --- Issue #520: zero-commit detection ---

#[tokio::test]
async fn auto_pr_zero_commits_skips_pr_with_visible_message() {
    let mock = MockGitHubClient::new();
    mock.set_create_pr_response(999); // would be visible if (wrongly) called
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    app = app.with_git_ops(Box::new(
        crate::git::MockGitOps::new().with_commits_ahead(false),
    ));
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec!["src/foo.rs".into()],
        Some("maestro/issue-42".into()),
        Some(std::path::PathBuf::from("/tmp/wt-42")),
        false,
    )
    .await;

    assert!(
        mock_handle.create_pr_calls().is_empty(),
        "create_pr must NOT be called when branch has no commits ahead (#520 AC2)"
    );
    assert!(
        app.pending_prs.is_empty(),
        "no retry must be queued for zero-commit sessions (#520 AC5)"
    );

    let has_zero_commits_log = app.activity_log.entries().iter().any(|e| {
        e.session_label == "#42"
            && e.message.contains("No commits found")
            && e.message.contains("maestro/issue-42")
            && matches!(e.level, LogLevel::Warn)
    });
    assert!(
        has_zero_commits_log,
        "must log Warn-level zero-commit message with branch name (#520 AC2)"
    );

    let has_critical_notification = app.notifications.all().iter().any(|n| {
        matches!(
            n.level,
            crate::notifications::types::InterruptLevel::Critical
        )
    });
    assert!(
        has_critical_notification,
        "must queue a Critical desktop notification for zero-commit sessions (#520 AC2)"
    );
}

#[tokio::test]
async fn auto_pr_git_check_error_falls_through_to_create_pr() {
    let mock = MockGitHubClient::new();
    mock.set_create_pr_response(101);
    let mock_handle = mock.clone();

    let mut app = make_app_with_mock(mock);
    app = app.with_git_ops(Box::new(crate::git::MockGitOps::new().with_failure()));
    app.state.issue_cache.insert(42, make_issue(42));

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.5,
        vec![],
        Some("maestro/issue-42".into()),
        Some(std::path::PathBuf::from("/tmp/wt-42")),
        false,
    )
    .await;

    assert_eq!(
        mock_handle.create_pr_calls().len(),
        1,
        "create_pr MUST still be called when has_commits_ahead errors — \
         AC3 fallthrough contract (#520)"
    );
}

#[tokio::test]
async fn auto_pr_when_auth_missing_enqueues_pending_pr_for_manual_retry() {
    use crate::provider::github::types::PendingPrStatus;

    let mock = MockGitHubClient::new();
    let mut app = make_app_with_mock(mock);
    // Simulate the documented #521 scenario: GitHub auth was missing
    // when the session ended. Once the user runs `gh auth login` and
    // presses Shift+P, the queued PendingPr drives the retry.
    app.gh_auth_ok = false;

    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        1.0,
        vec!["src/foo.rs".into()],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;

    assert_eq!(
        app.pending_prs.len(),
        1,
        "auth-missing path MUST enqueue a PendingPr so Shift+P can recover (#521)",
    );
    let p = &app.pending_prs[0];
    assert_eq!(p.issue_number, 42);
    assert_eq!(p.branch, "maestro/issue-42");
    assert_eq!(p.status, PendingPrStatus::AwaitingManualRetry);
    assert!(p.next_retry_at.is_none(), "manual retry — no auto schedule");
    assert!(
        p.last_errors.back().is_some_and(|s| s.contains("auth")),
        "last_errors.back() must explain auth missing, got: {:?}",
        p.last_errors,
    );
    assert_eq!(p.attempt, 0);

    // Activity log should explicitly mention the recovery action.
    let warn_msg = app
        .activity_log
        .entries()
        .iter()
        .rev()
        .find(|e| matches!(e.level, LogLevel::Warn) && e.session_label == "#42")
        .expect("warn entry for #42 must exist");
    assert!(
        warn_msg.message.contains("auth missing"),
        "activity log entry should reference auth-missing, got: {}",
        warn_msg.message,
    );
    assert!(
        warn_msg.message.contains("Shift+P"),
        "activity log entry should tell user to press Shift+P, got: {}",
        warn_msg.message,
    );
}

#[tokio::test]
async fn auto_pr_when_auth_missing_without_branch_skips_pending_pr() {
    let mock = MockGitHubClient::new();
    let mut app = make_app_with_mock(mock);
    app.gh_auth_ok = false;

    // No worktree branch — there is no PR to retry, so the auth-skip
    // path must NOT enqueue a doomed PendingPr.
    app.on_issue_session_completed(42, vec![42], true, 0.0, vec![], None, None, false)
        .await;

    assert!(
        app.pending_prs.is_empty(),
        "no branch = nothing to retry; PendingPr must not be created",
    );
}

#[tokio::test]
async fn auto_pr_when_auth_missing_does_not_double_enqueue() {
    let mock = MockGitHubClient::new();
    let mut app = make_app_with_mock(mock);
    app.gh_auth_ok = false;

    // First call enqueues.
    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.0,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;
    assert_eq!(app.pending_prs.len(), 1);

    // Second call (e.g., session re-emitted Completed event) must not
    // append a duplicate.
    app.on_issue_session_completed(
        42,
        vec![42],
        true,
        0.0,
        vec![],
        Some("maestro/issue-42".into()),
        None,
        false,
    )
    .await;
    assert_eq!(
        app.pending_prs.len(),
        1,
        "auth-missing path must dedupe by issue_number",
    );
}
