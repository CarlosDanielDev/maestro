use super::super::pr::{parse_pr_json, parse_prs_json};
use super::{MockAzRunner, test_client};
use crate::provider::github::client::RepoProvider;
use crate::provider::types::ReviewEvent;
use std::sync::Arc;

#[test]
fn parse_prs_json_maps_azure_devops_fields() {
    let prs = parse_prs_json(
        r#"[{
            "pullRequestId": 42,
            "title": "Add login",
            "description": "Implements login",
            "sourceRefName": "refs/heads/feature/login",
            "targetRefName": "refs/heads/main",
            "createdBy": {"uniqueName": "dev@example.com"},
            "status": "active",
            "url": "https://dev.azure.com/example/Project/_git/repo/pullrequest/42"
        }]"#,
    )
    .unwrap();

    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].number, 42);
    assert_eq!(prs[0].title, "Add login");
    assert_eq!(prs[0].body, "Implements login");
    assert_eq!(prs[0].head_branch, "refs/heads/feature/login");
    assert_eq!(prs[0].base_branch, "refs/heads/main");
    assert_eq!(prs[0].author, "dev@example.com");
    assert_eq!(prs[0].state, "active");
    assert_eq!(
        prs[0].html_url,
        "https://dev.azure.com/example/Project/_git/repo/pullrequest/42"
    );
}

#[test]
fn parse_pr_json_defaults_missing_fields() {
    let pr = parse_pr_json(r#"{"pullRequestId":7}"#).unwrap();

    assert_eq!(pr.number, 7);
    assert_eq!(pr.title, "");
    assert_eq!(pr.body, "");
    assert_eq!(pr.head_branch, "");
    assert_eq!(pr.base_branch, "");
    assert_eq!(pr.author, "");
    assert_eq!(pr.state, "");
    assert_eq!(pr.html_url, "");
    assert!(pr.labels.is_empty());
    assert!(!pr.draft);
    assert!(!pr.mergeable);
}

#[test]
fn parse_prs_json_invalid_json_returns_err() {
    assert!(parse_prs_json("{not json}").is_err());
}

#[tokio::test]
async fn list_open_prs_calls_az_and_parses_results() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"[{
            "pullRequestId": 11,
            "title": "Title",
            "description": "Body",
            "sourceRefName": "refs/heads/feature",
            "targetRefName": "refs/heads/main",
            "createdBy": {"uniqueName": "author@example.com"},
            "status": "active",
            "url": "https://dev.azure.com/example/Project/_git/repo/pullrequest/11"
        }]"#,
    )]));
    let client = test_client(runner.clone());

    let prs = client.list_open_prs().await.unwrap();

    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].number, 11);
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        vec![
            "repos",
            "pr",
            "list",
            "--status",
            "active",
            "--org",
            "https://dev.azure.com/example",
            "--project",
            "Project",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn get_pr_calls_az_show_and_parses_result() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{
            "pullRequestId": 12,
            "title": "Title",
            "description": "Body",
            "sourceRefName": "refs/heads/feature",
            "targetRefName": "refs/heads/main",
            "createdBy": {"uniqueName": "author@example.com"},
            "status": "active",
            "url": "https://dev.azure.com/example/Project/_git/repo/pullrequest/12"
        }"#,
    )]));
    let client = test_client(runner.clone());

    let pr = client.get_pr(12).await.unwrap();

    assert_eq!(pr.number, 12);
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        vec![
            "repos",
            "pr",
            "show",
            "--id",
            "12",
            "--org",
            "https://dev.azure.com/example",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn submit_pr_review_comment_posts_comment_without_vote() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("")]));
    let client = test_client(runner.clone());

    client
        .submit_pr_review(13, ReviewEvent::Comment, "Looks good")
        .await
        .unwrap();

    assert_eq!(
        runner.calls(),
        vec![vec![
            "repos",
            "pr",
            "comment",
            "add",
            "--id",
            "13",
            "--content",
            "Looks good",
            "--org",
            "https://dev.azure.com/example",
            "--project",
            "Project"
        ]]
    );
}

#[tokio::test]
async fn submit_pr_review_approve_votes_then_comments() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(""),
        MockAzRunner::success(""),
    ]));
    let client = test_client(runner.clone());

    client
        .submit_pr_review(14, ReviewEvent::Approve, "Approved")
        .await
        .unwrap();

    assert_eq!(
        runner.calls(),
        vec![
            vec![
                "repos",
                "pr",
                "set-vote",
                "--id",
                "14",
                "--vote",
                "approve",
                "--org",
                "https://dev.azure.com/example",
                "--project",
                "Project"
            ],
            vec![
                "repos",
                "pr",
                "comment",
                "add",
                "--id",
                "14",
                "--content",
                "Approved",
                "--org",
                "https://dev.azure.com/example",
                "--project",
                "Project"
            ]
        ]
    );
}

#[tokio::test]
async fn submit_pr_review_request_changes_votes_reject_then_comments() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(""),
        MockAzRunner::success(""),
    ]));
    let client = test_client(runner.clone());

    client
        .submit_pr_review(15, ReviewEvent::RequestChanges, "Needs changes")
        .await
        .unwrap();

    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].windows(2).any(|w| w == ["--vote", "reject"]));
    assert!(
        calls[1]
            .windows(2)
            .any(|w| w == ["--content", "Needs changes"])
    );
}
