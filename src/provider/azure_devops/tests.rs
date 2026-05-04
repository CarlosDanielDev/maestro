use super::issues::{azure_tags_field, build_create_work_item_args, parse_created_work_item_id};
use super::*;
use crate::provider::github::client::RepoProvider;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct MockAzRunner {
    calls: Mutex<Vec<Vec<String>>>,
    outputs: Mutex<VecDeque<AzOutput>>,
}

impl MockAzRunner {
    fn new(outputs: Vec<AzOutput>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
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
}

#[async_trait]
impl AzRunner for MockAzRunner {
    async fn run(&self, args: &[&str]) -> Result<AzOutput> {
        self.calls.lock().unwrap().push(
            args.iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>(),
        );
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
async fn create_issue_omits_iteration_when_milestone_is_missing() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{"id":125}"#,
    )]));
    let client = test_client(runner.clone());

    client
        .create_issue("Title", "Body", &[], Some(99))
        .await
        .unwrap();

    assert!(!runner.calls()[0].iter().any(|arg| arg == "--iteration"));
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
