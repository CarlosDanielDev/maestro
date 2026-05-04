use super::*;
use crate::provider::github::transport::GitHubClient;
use crate::provider::github::types::GhPullRequest;

fn make_pr(number: u64) -> GhPullRequest {
    GhPullRequest {
        number,
        title: format!("PR #{}", number),
        body: String::new(),
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/pull/{}", number),
        head_branch: format!("fix/issue-{}", number),
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
async fn mock_list_open_prs_returns_configured_prs() {
    let client = MockGitHubClient::new();
    client.set_pull_requests(vec![make_pr(10), make_pr(11)]);
    let prs = client.list_open_prs().await.unwrap();
    assert_eq!(prs.len(), 2);
    assert_eq!(prs[0].number, 10);
    assert_eq!(prs[1].number, 11);
}

#[tokio::test]
async fn mock_list_open_prs_returns_empty_by_default() {
    let client = MockGitHubClient::new();
    let prs = client.list_open_prs().await.unwrap();
    assert!(prs.is_empty());
}

#[tokio::test]
async fn mock_list_open_prs_propagates_configured_error() {
    let client = MockGitHubClient::new();
    client.set_list_open_prs_error("connection refused");
    let result = client.list_open_prs().await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("connection refused")
    );
}

#[tokio::test]
async fn mock_get_pr_returns_pr_by_number() {
    let client = MockGitHubClient::new();
    client.set_pull_requests(vec![make_pr(42)]);
    let pr = client.get_pr(42).await.unwrap();
    assert_eq!(pr.number, 42);
}

#[tokio::test]
async fn mock_get_pr_returns_not_found_for_missing_number() {
    let client = MockGitHubClient::new();
    let result = client.get_pr(99).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mock_get_pr_propagates_configured_error() {
    let client = MockGitHubClient::new();
    client.set_get_pr_error(5, "rate limited");
    client.set_pull_requests(vec![make_pr(5)]);
    let result = client.get_pr(5).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("rate limited"));
}

#[tokio::test]
async fn mock_submit_pr_review_records_approve_call() {
    use crate::provider::github::types::PrReviewEvent;
    let client = MockGitHubClient::new();
    client
        .submit_pr_review(7, PrReviewEvent::Approve, "LGTM")
        .await
        .unwrap();
    let calls = client.submit_pr_review_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].pr_number, 7);
    assert_eq!(calls[0].event, PrReviewEvent::Approve);
    assert_eq!(calls[0].body, "LGTM");
}

#[tokio::test]
async fn mock_submit_pr_review_records_request_changes_call() {
    use crate::provider::github::types::PrReviewEvent;
    let client = MockGitHubClient::new();
    client
        .submit_pr_review(3, PrReviewEvent::RequestChanges, "needs work")
        .await
        .unwrap();
    let calls = client.submit_pr_review_calls();
    assert_eq!(calls[0].event, PrReviewEvent::RequestChanges);
}

#[tokio::test]
async fn mock_submit_pr_review_records_comment_call() {
    use crate::provider::github::types::PrReviewEvent;
    let client = MockGitHubClient::new();
    client
        .submit_pr_review(1, PrReviewEvent::Comment, "nice")
        .await
        .unwrap();
    let calls = client.submit_pr_review_calls();
    assert_eq!(calls[0].event, PrReviewEvent::Comment);
}

#[tokio::test]
async fn mock_submit_pr_review_propagates_configured_error() {
    use crate::provider::github::types::PrReviewEvent;
    let client = MockGitHubClient::new();
    client.set_submit_pr_review_error("forbidden");
    let result = client.submit_pr_review(1, PrReviewEvent::Approve, "").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("forbidden"));
}
