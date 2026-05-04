use super::super::ci::{aggregate_pipeline_runs, parse_pipeline_runs_json};
use super::super::merge::merge_method_args;
use super::{MockAzRunner, test_client};
use crate::provider::github::client::RepoProvider;
use crate::provider::types::{CiStatus, MergeMethod};
use std::sync::Arc;

#[test]
fn parse_pipeline_runs_accepts_value_wrapper() {
    let runs = parse_pipeline_runs_json(
        r#"{
            "value": [{
                "id": 100,
                "status": "completed",
                "result": "succeeded",
                "definition": {"id": 7, "name": "ci"}
            }]
        }"#,
    )
    .unwrap();

    assert_eq!(runs.len(), 1);
}

#[test]
fn aggregate_pipeline_runs_passes_when_latest_per_definition_succeeded() {
    let runs = parse_pipeline_runs_json(
        r#"[
            {
                "id": 101,
                "createdDate": "2026-05-04T10:00:00Z",
                "status": "completed",
                "result": "failed",
                "definition": {"id": 1, "name": "linux"}
            },
            {
                "id": 102,
                "createdDate": "2026-05-04T11:00:00Z",
                "status": "completed",
                "result": "succeeded",
                "definition": {"id": 1, "name": "linux"}
            },
            {
                "id": 103,
                "createdDate": "2026-05-04T11:00:00Z",
                "status": "completed",
                "result": "succeeded",
                "definition": {"id": 2, "name": "windows"}
            }
        ]"#,
    )
    .unwrap();

    assert_eq!(aggregate_pipeline_runs(runs), CiStatus::Passed);
}

#[test]
fn aggregate_pipeline_runs_failed_summary_counts_all_states() {
    let runs = parse_pipeline_runs_json(
        r#"[
            {
                "id": 201,
                "status": "completed",
                "result": "failed",
                "definition": {"id": 1, "name": "linux"}
            },
            {
                "id": 202,
                "status": "completed",
                "result": "succeeded",
                "definition": {"id": 2, "name": "windows"}
            },
            {
                "id": 203,
                "status": "inProgress",
                "definition": {"id": 3, "name": "macos"}
            },
            {
                "id": 204,
                "status": "notStarted",
                "definition": {"id": 4, "name": "docs"}
            }
        ]"#,
    )
    .unwrap();

    match aggregate_pipeline_runs(runs) {
        CiStatus::Failed { summary } => {
            assert!(summary.contains("1 failed"));
            assert!(summary.contains("1 passed"));
            assert!(summary.contains("1 in progress"));
            assert!(summary.contains("1 pending"));
            assert!(summary.contains("linux"));
        }
        other => panic!("expected failed status, got {other:?}"),
    }
}

#[test]
fn aggregate_pipeline_runs_pending_when_no_failures_are_still_running() {
    let runs = parse_pipeline_runs_json(
        r#"[{
            "id": 301,
            "status": "inProgress",
            "definition": {"id": 1, "name": "linux"}
        }]"#,
    )
    .unwrap();

    assert_eq!(aggregate_pipeline_runs(runs), CiStatus::Pending);
}

#[test]
fn aggregate_pipeline_runs_none_configured_for_empty_runs() {
    assert_eq!(
        aggregate_pipeline_runs(Vec::new()),
        CiStatus::NoneConfigured
    );
}

#[tokio::test]
async fn ci_status_for_branch_calls_az_pipeline_runs_list() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"[{
            "id": 401,
            "status": "completed",
            "result": "succeeded",
            "definition": {"id": 1, "name": "ci"}
        }]"#,
    )]));
    let client = test_client(runner.clone());

    assert_eq!(
        client.ci_status_for_branch("feature/login").await.unwrap(),
        CiStatus::Passed
    );

    assert_eq!(
        runner.calls(),
        vec![vec![
            "pipelines",
            "runs",
            "list",
            "--branch",
            "refs/heads/feature/login",
            "--top",
            "20",
            "--org",
            "https://dev.azure.com/example",
            "--project",
            "Project",
            "-o",
            "json"
        ]]
    );
}

#[tokio::test]
async fn ci_status_for_pr_resolves_source_branch() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(
            r#"{
                "pullRequestId": 44,
                "sourceRefName": "refs/heads/feature/login",
                "status": "active"
            }"#,
        ),
        MockAzRunner::success(
            r#"[{
                "id": 402,
                "status": "completed",
                "result": "succeeded",
                "definition": {"id": 1, "name": "ci"}
            }]"#,
        ),
    ]));
    let client = test_client(runner.clone());

    assert_eq!(client.ci_status_for_pr(44).await.unwrap(), CiStatus::Passed);

    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert!(
        calls[1]
            .windows(2)
            .any(|w| w == ["--branch", "refs/heads/feature/login"])
    );
}

#[tokio::test]
async fn ci_logs_for_check_uses_query_logs() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(r#"{"id":55}"#),
        MockAzRunner::success("raw log text"),
    ]));
    let client = test_client(runner.clone());

    assert_eq!(
        client.ci_logs_for_check("55").await.unwrap(),
        "raw log text"
    );
    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].windows(3).any(|w| w == ["runs", "show", "--id"]));
    assert!(calls[1].windows(2).any(|w| w == ["--query", "logs"]));
}

#[tokio::test]
async fn ci_logs_for_check_falls_back_to_build_logs_resource() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(r#"{"id":56}"#),
        MockAzRunner::failure("query failed"),
        MockAzRunner::success("fallback log text"),
    ]));
    let client = test_client(runner.clone());

    assert_eq!(
        client.ci_logs_for_check("56").await.unwrap(),
        "fallback log text"
    );
    assert!(
        runner.calls()[2]
            .windows(2)
            .any(|w| w == ["--resource", "logs"])
    );
}

#[test]
fn merge_method_mapping_matches_azure_devops_flags() {
    assert_eq!(
        merge_method_args(MergeMethod::Squash),
        vec!["--squash", "true"]
    );
    assert_eq!(
        merge_method_args(MergeMethod::Rebase),
        vec!["--merge-strategy", "rebase"]
    );
    assert_eq!(
        merge_method_args(MergeMethod::Merge),
        vec!["--merge-strategy", "noFastForward"]
    );
}

#[tokio::test]
async fn merge_pr_squash_completes_active_pr() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(
            r#"{
                "pullRequestId": 57,
                "sourceRefName": "refs/heads/feature/login",
                "status": "active",
                "isDraft": false
            }"#,
        ),
        MockAzRunner::success("{}"),
    ]));
    let client = test_client(runner.clone());

    client.merge_pr(57, MergeMethod::Squash).await.unwrap();

    assert_eq!(
        runner.calls()[1],
        vec![
            "repos",
            "pr",
            "update",
            "--id",
            "57",
            "--status",
            "completed",
            "--merge-commit-message-mode",
            "default",
            "--squash",
            "true",
            "--org",
            "https://dev.azure.com/example",
            "--project",
            "Project"
        ]
    );
}

#[tokio::test]
async fn merge_pr_rejects_draft_before_update() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{
            "pullRequestId": 58,
            "status": "active",
            "isDraft": true
        }"#,
    )]));
    let client = test_client(runner.clone());

    let err = client
        .merge_pr(58, MergeMethod::Merge)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("draft"));
    assert_eq!(runner.calls().len(), 1);
}
