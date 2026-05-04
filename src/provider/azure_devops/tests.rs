use super::issues::{azure_tags_field, build_create_work_item_args, parse_created_work_item_id};
use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

mod ci_merge;
mod issue_creation;
mod iteration_creation;
mod iterations;
mod pull_requests;
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
