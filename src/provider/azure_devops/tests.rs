use super::issues::{azure_tags_field, build_create_work_item_args, parse_created_work_item_id};
use super::pr::{parse_pr_json, parse_prs_json};
use super::*;
use crate::provider::github::client::RepoProvider;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

mod iterations;
mod tags;

struct MockAzRunner {
    calls: Mutex<Vec<Vec<String>>>,
    in_file_contents: Mutex<Vec<String>>,
    outputs: Mutex<VecDeque<AzOutput>>,
}

impl MockAzRunner {
    fn new(outputs: Vec<AzOutput>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            in_file_contents: Mutex::new(Vec::new()),
            outputs: Mutex::new(outputs.into()),
        }
    }

    fn success(stdout: &str) -> AzOutput {
        AzOutput {
            success: true,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn failure(stderr: &str) -> AzOutput {
        AzOutput {
            success: false,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    fn in_file_contents(&self) -> Vec<String> {
        self.in_file_contents.lock().unwrap().clone()
    }
}

#[async_trait]
impl AzRunner for MockAzRunner {
    async fn run(&self, args: &[&str]) -> Result<AzOutput> {
        self.calls.lock().unwrap().push(
            args.iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>(),
        );
        if let Some(path) = args
            .windows(2)
            .find_map(|window| (window[0] == "--in-file").then_some(window[1]))
        {
            self.in_file_contents
                .lock()
                .unwrap()
                .push(std::fs::read_to_string(path)?);
        }
        self.outputs
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("no mock az output queued"))
    }
}

fn test_client(runner: Arc<dyn AzRunner>) -> AzDevOpsClient {
    AzDevOpsClient::with_runner(
        "https://dev.azure.com/example".to_string(),
        "Project".to_string(),
        runner,
    )
}

#[test]
fn parse_work_items_json_valid_single_item() {
    let json = r#"[{
        "id": 101,
        "fields": {
            "System.Title": "Fix login bug",
            "System.Description": "Detailed description",
            "System.State": "Active",
            "System.Tags": "maestro:ready; priority:P1"
        },
        "url": "https://dev.azure.com/MyOrg/MyProject/_apis/wit/workItems/101"
    }]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].number, 101);
    assert_eq!(issues[0].title, "Fix login bug");
    assert_eq!(issues[0].state, "open");
}

#[test]
fn parse_work_items_json_maps_labels_from_tags() {
    let json = r#"[{
        "id": 1,
        "fields": {
            "System.Title": "T",
            "System.State": "Active",
            "System.Tags": "maestro:ready; priority:P1"
        },
        "url": ""
    }]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert_eq!(issues[0].labels, vec!["maestro:ready", "priority:P1"]);
}

#[test]
fn parse_work_items_json_empty_tags_produces_empty_labels() {
    let json = r#"[{
        "id": 1,
        "fields": {"System.Title": "T", "System.State": "Active", "System.Tags": ""},
        "url": ""
    }]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert!(issues[0].labels.is_empty());
}

#[test]
fn parse_work_items_json_active_state_maps_to_open() {
    let json = r#"[{"id":1,"fields":{"System.Title":"T","System.State":"Active"},"url":""}]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert_eq!(issues[0].state, "open");
}

#[test]
fn parse_work_items_json_closed_state_maps_to_closed() {
    let json = r#"[{"id":1,"fields":{"System.Title":"T","System.State":"Closed"},"url":""}]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert_eq!(issues[0].state, "closed");
}

#[test]
fn parse_work_items_json_resolved_state_maps_to_closed() {
    let json = r#"[{"id":1,"fields":{"System.Title":"T","System.State":"Resolved"},"url":""}]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert_eq!(issues[0].state, "closed");
}

#[test]
fn parse_work_items_json_empty_array() {
    let issues = parse_work_items_json("[]").unwrap();
    assert!(issues.is_empty());
}

#[test]
fn parse_work_items_json_invalid_json_returns_err() {
    assert!(parse_work_items_json("{not json}").is_err());
}

#[test]
fn parse_work_items_json_missing_id_returns_err() {
    let json = r#"[{"fields":{"System.Title":"T","System.State":"Active"},"url":""}]"#;
    assert!(parse_work_items_json(json).is_err());
}

#[test]
fn parse_work_items_json_captures_url() {
    let json = r#"[{
        "id": 42,
        "fields": {"System.Title": "T", "System.State": "Active"},
        "url": "https://dev.azure.com/MyOrg/MyProject/_apis/wit/workItems/42"
    }]"#;
    let issues = parse_work_items_json(json).unwrap();
    assert_eq!(
        issues[0].html_url,
        "https://dev.azure.com/MyOrg/MyProject/_apis/wit/workItems/42"
    );
}

#[test]
fn azure_tags_field_joins_labels_with_semicolon_space() {
    let labels = vec!["maestro:ready".to_string(), "priority:P1".to_string()];
    assert_eq!(
        azure_tags_field(&labels).unwrap(),
        "System.Tags=maestro:ready; priority:P1"
    );
}

#[test]
fn azure_tags_field_omits_empty_labels() {
    assert!(azure_tags_field(&[]).is_none());
}

#[test]
fn build_create_work_item_args_includes_iteration_path() {
    let labels = vec!["a".to_string(), "b".to_string()];
    let args = build_create_work_item_args(
        "https://dev.azure.com/example",
        "Project",
        "Title",
        "Body",
        &labels,
        Some("Project\\Sprint 1"),
    );

    assert!(
        args.windows(2)
            .any(|w| w == ["--iteration", "Project\\Sprint 1"])
    );
    assert!(
        args.windows(2)
            .any(|w| w == ["--fields", "System.Tags=a; b"])
    );
}

#[test]
fn build_create_work_item_args_omits_iteration_when_none() {
    let args = build_create_work_item_args(
        "https://dev.azure.com/example",
        "Project",
        "Title",
        "Body",
        &[],
        None,
    );

    assert!(!args.iter().any(|arg| arg == "--iteration"));
    assert!(!args.iter().any(|arg| arg == "--fields"));
}

#[test]
fn parse_created_work_item_id_reads_id() {
    assert_eq!(parse_created_work_item_id(r#"{"id":321}"#).unwrap(), 321);
}

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
async fn create_issue_happy_path_returns_created_id() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{"id":123}"#,
    )]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_issue("  New   story  ", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(outcome, CreateOutcome::Created(123));
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].windows(2).any(|w| w == ["--title", "New story"]));
    assert!(calls[0].windows(2).any(|w| w == ["--description", "Body"]));
    assert!(calls[0].windows(2).any(|w| w == ["--type", "User Story"]));
    assert!(
        calls[0]
            .windows(2)
            .any(|w| w == ["--org", "https://dev.azure.com/example"])
    );
    assert!(calls[0].windows(2).any(|w| w == ["--project", "Project"]));
}

#[tokio::test]
async fn create_issue_sends_semicolon_joined_labels() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{"id":124}"#,
    )]));
    let client = test_client(runner.clone());
    let labels = vec!["maestro:ready".to_string(), "priority:P1".to_string()];

    client
        .create_issue("Title", "Body", &labels, None)
        .await
        .unwrap();

    assert!(
        runner.calls()[0]
            .windows(2)
            .any(|w| w == ["--fields", "System.Tags=maestro:ready; priority:P1"])
    );
}

#[tokio::test]
async fn create_issue_errors_when_requested_milestone_is_missing() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("[]")]));
    let client = test_client(runner.clone());

    let err = client
        .create_issue("Title", "Body", &[], Some(99))
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("Azure DevOps iteration for milestone id 99 not found"));
    assert_eq!(runner.calls().len(), 1);
}

#[tokio::test]
async fn create_issue_propagates_az_stderr() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::failure(
        "VS403474: malformed field value",
    )]));
    let client = test_client(runner);

    let err = client
        .create_issue("Title", "Body", &[], None)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("VS403474: malformed field value"));
}

#[tokio::test]
async fn create_issue_rejects_invalid_title_before_api_call() {
    let runner = Arc::new(MockAzRunner::new(Vec::new()));
    let client = test_client(runner.clone());

    let err = client
        .create_issue("   ", "Body", &[], None)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("issue title must not be empty"));
    assert!(runner.calls().is_empty());
}

#[tokio::test]
async fn create_issue_rejects_overlong_title_before_api_call() {
    let runner = Arc::new(MockAzRunner::new(Vec::new()));
    let client = test_client(runner.clone());
    let title = "x".repeat(257);

    let err = client
        .create_issue(&title, "Body", &[], None)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("issue title too long"));
    assert!(runner.calls().is_empty());
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

#[tokio::test]
async fn merge_pr_stub_names_tracking_issue() {
    let client = AzDevOpsClient::new(
        "https://dev.azure.com/example".to_string(),
        "Project".to_string(),
    );

    let err = client
        .merge_pr(123, MergeMethod::Squash)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("v0.23.0 #B5"));
}
