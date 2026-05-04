use super::*;
use crate::provider::github::transport::GitHubClient;
use crate::provider::github::types::{GhIssue, GhMilestone};

fn make_issue(number: u64, labels: &[&str]) -> GhIssue {
    GhIssue {
        number,
        title: format!("Issue #{}", number),
        body: String::new(),
        labels: labels.iter().map(|s| s.to_string()).collect(),
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: None,
        assignees: vec![],
    }
}

// MockGitHubClient tests

#[tokio::test]
async fn mock_list_issues_returns_all_when_no_filter() {
    let client = MockGitHubClient::new();
    client.set_issues(vec![
        make_issue(1, &["maestro:ready"]),
        make_issue(2, &["bug"]),
    ]);
    let issues = client.list_issues(&[]).await.unwrap();
    assert_eq!(issues.len(), 2);
}

#[tokio::test]
async fn mock_list_issues_filters_by_label() {
    let client = MockGitHubClient::new();
    client.set_issues(vec![
        make_issue(1, &["maestro:ready"]),
        make_issue(2, &["bug"]),
    ]);
    let issues = client.list_issues(&["maestro:ready"]).await.unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].number, 1);
}

#[tokio::test]
async fn mock_get_issue_found() {
    let client = MockGitHubClient::new();
    client.set_issues(vec![make_issue(42, &["maestro:ready"])]);
    let issue = client.get_issue(42).await.unwrap();
    assert_eq!(issue.number, 42);
}

#[tokio::test]
async fn mock_get_issue_not_found_returns_err() {
    let client = MockGitHubClient::new();
    let result = client.get_issue(999).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mock_get_issue_custom_error() {
    let client = MockGitHubClient::new();
    client.set_get_issue_error(10, "rate limited");
    client.set_issues(vec![make_issue(10, &[])]);
    let err = client.get_issue(10).await.unwrap_err();
    assert!(err.to_string().contains("rate limited"));
}

#[tokio::test]
async fn mock_add_label_records_call() {
    let client = MockGitHubClient::new();
    client.add_label(7, "maestro:in-progress").await.unwrap();
    let calls = client.add_label_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], (7, "maestro:in-progress".to_string()));
}

#[tokio::test]
async fn mock_add_label_propagates_configured_error() {
    let client = MockGitHubClient::new();
    client.set_add_label_error("label not found");
    let result = client.add_label(1, "anything").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("label not found"));
}

#[tokio::test]
async fn mock_remove_label_records_call() {
    let client = MockGitHubClient::new();
    client.remove_label(5, "maestro:ready").await.unwrap();
    let calls = client.remove_label_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], (5, "maestro:ready".to_string()));
}

#[tokio::test]
async fn mock_remove_label_propagates_configured_error() {
    let client = MockGitHubClient::new();
    client.set_remove_label_error("network error");
    let result = client.remove_label(1, "anything").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mock_create_pr_records_call() {
    let client = MockGitHubClient::new();
    client.set_create_pr_response(42);
    let pr_number = client
        .create_pr(
            10,
            "feat: add thing",
            "Closes #10",
            "maestro/issue-10",
            "main",
        )
        .await
        .unwrap();
    assert_eq!(pr_number, 42);

    let calls = client.create_pr_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].issue_number, 10);
    assert_eq!(calls[0].head_branch, "maestro/issue-10");
    assert_eq!(calls[0].base_branch, "main");
}

#[tokio::test]
async fn mock_create_pr_propagates_configured_error() {
    let client = MockGitHubClient::new();
    client.set_create_pr_error("branch not found");
    let result = client
        .create_pr(1, "title", "body", "bad-branch", "main")
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("branch not found"));
}

// MockGitHubClient::list_milestones

#[tokio::test]
async fn mock_list_milestones_returns_stored_milestones() {
    let client = MockGitHubClient::new();
    client.set_milestones(vec![
        GhMilestone {
            number: 1,
            title: "v1.0".to_string(),
            description: String::new(),
            state: "open".to_string(),
            open_issues: 2,
            closed_issues: 3,
        },
        GhMilestone {
            number: 2,
            title: "v2.0".to_string(),
            description: String::new(),
            state: "open".to_string(),
            open_issues: 0,
            closed_issues: 0,
        },
    ]);
    let milestones = client.list_milestones("open").await.unwrap();
    assert_eq!(milestones.len(), 2);
    assert_eq!(milestones[0].title, "v1.0");
}

#[tokio::test]
async fn mock_list_milestones_returns_empty_when_none_set() {
    let client = MockGitHubClient::new();
    let milestones = client.list_milestones("open").await.unwrap();
    assert!(milestones.is_empty());
}

// -- list_prs_for_branch --

#[tokio::test]
async fn mock_list_prs_for_branch_returns_configured_prs() {
    let client = MockGitHubClient::new();
    client.set_list_prs_for_branch("maestro/issue-42", vec![10, 20]);
    let prs = client
        .list_prs_for_branch("maestro/issue-42")
        .await
        .unwrap();
    assert_eq!(prs, vec![10, 20]);
}

#[tokio::test]
async fn mock_list_prs_for_branch_returns_empty_for_unknown_branch() {
    let client = MockGitHubClient::new();
    let prs = client
        .list_prs_for_branch("maestro/issue-99")
        .await
        .unwrap();
    assert!(prs.is_empty());
}

// -- create_milestone / create_issue mock tests --

#[tokio::test]
async fn mock_create_milestone_records_call_and_returns_number() {
    let client = MockGitHubClient::new();
    let o1 = client
        .create_milestone("M0", "First milestone")
        .await
        .unwrap();
    let o2 = client
        .create_milestone("M1", "Second milestone")
        .await
        .unwrap();
    assert_eq!(o1, CreateOutcome::Created(1));
    assert_eq!(o2, CreateOutcome::Created(2));
    let calls = client.create_milestone_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "M0");
    assert_eq!(calls[1].0, "M1");
}

#[tokio::test]
async fn mock_create_issue_records_call_and_returns_number() {
    let client = MockGitHubClient::new();
    let o = client
        .create_issue("feat: thing", "body", &["enhancement".into()], Some(1))
        .await
        .unwrap();
    assert_eq!(o, CreateOutcome::Created(1));
    let calls = client.create_issue_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].title, "feat: thing");
    assert_eq!(calls[0].labels, vec!["enhancement"]);
    assert_eq!(calls[0].milestone, Some(1));
}

#[tokio::test]
async fn mock_create_issue_increments_counter() {
    let client = MockGitHubClient::new();
    let o1 = client.create_issue("a", "", &[], None).await.unwrap();
    let o2 = client.create_issue("b", "", &[], None).await.unwrap();
    let o3 = client.create_issue("c", "", &[], None).await.unwrap();
    assert_eq!(o1.number(), 1);
    assert_eq!(o2.number(), 2);
    assert_eq!(o3.number(), 3);
    assert!(!o1.is_existed());
    assert!(!o2.is_existed());
    assert!(!o3.is_existed());
}
