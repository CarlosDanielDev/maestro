use super::{MockAzRunner, test_client};
use crate::provider::azure_devops::parse_tags_json;
use crate::provider::github::client::RepoProvider;
use std::sync::Arc;

#[test]
fn parse_tags_json_reads_value_names() {
    let json = r#"{
        "count": 2,
        "value": [
            {"id": "1", "name": "maestro:ready", "active": true},
            {"id": "2", "name": "priority:P1", "active": true}
        ]
    }"#;

    let tags = parse_tags_json(json).unwrap();

    assert_eq!(tags, vec!["maestro:ready", "priority:P1"]);
}

#[test]
fn parse_tags_json_empty_project_returns_empty_vec() {
    let tags = parse_tags_json(r#"{"count":0,"value":[]}"#).unwrap();

    assert!(tags.is_empty());
}

#[test]
fn parse_tags_json_invalid_json_returns_err() {
    assert!(parse_tags_json("{not json}").is_err());
}

#[tokio::test]
async fn list_labels_invokes_work_item_tags_endpoint() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{"count":1,"value":[{"name":"maestro:ready"}]}"#,
    )]));
    let client = test_client(runner.clone());

    let labels = client.list_labels().await.unwrap();

    assert_eq!(labels, vec!["maestro:ready"]);
    assert_eq!(
        runner.calls()[0],
        vec![
            "devops",
            "invoke",
            "--area",
            "wit",
            "--resource",
            "tags",
            "--route-parameters",
            "project=Project",
            "--org",
            "https://dev.azure.com/example",
            "-o",
            "json",
        ]
    );
}

#[tokio::test]
async fn create_label_patches_work_item_tags_endpoint() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("{}")]));
    let client = test_client(runner.clone());

    client
        .create_label("maestro:ready", "0E8A16")
        .await
        .unwrap();

    let calls = runner.calls();
    assert!(calls[0].windows(2).any(|w| w == ["--area", "wit"]));
    assert!(calls[0].windows(2).any(|w| w == ["--resource", "tags"]));
    assert!(
        calls[0]
            .windows(2)
            .any(|w| w == ["--route-parameters", "project=Project"])
    );
    assert!(calls[0].windows(2).any(|w| w == ["--http-method", "PATCH"]));
    assert!(calls[0].iter().any(|arg| arg == "--in-file"));
    assert_eq!(
        runner.in_file_contents(),
        vec![r#"{"name":"maestro:ready"}"#]
    );
}

#[tokio::test]
async fn create_label_duplicate_tag_error_returns_ok() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::failure(
        "TF401289: tag already exists",
    )]));
    let client = test_client(runner.clone());

    client
        .create_label("maestro:ready", "0E8A16")
        .await
        .unwrap();

    assert_eq!(runner.calls().len(), 1);
}
