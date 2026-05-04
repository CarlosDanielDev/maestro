use super::{MockAzRunner, test_client};
use crate::provider::azure_devops::iterations::stable_iteration_number;
use crate::provider::github::client::{CreateOutcome, RepoProvider};
use std::sync::Arc;

#[tokio::test]
async fn create_milestone_duplicate_title_returns_existed_without_create() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"[{
            "name": "Sprint 2",
            "path": "Project\\Sprint 2",
            "attributes": { "finishDate": "2026-06-04T00:00:00Z" }
        }]"#,
    )]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_milestone("  sprint   2  ", "desc")
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: stable_iteration_number("Project\\Sprint 2"),
            state: "open".to_string()
        }
    );
    assert_eq!(runner.calls().len(), 1);
    assert!(!runner.calls().iter().flatten().any(|arg| arg == "create"));
}

#[tokio::test]
async fn create_milestone_creates_iteration_updates_description_and_returns_stable_id() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(
            r#"{
                "name": "Sprint 2",
                "path": "Project\\Sprint 2",
                "attributes": { "finishDate": "2026-06-04T00:00:00Z" }
            }"#,
        ),
        MockAzRunner::success(
            r#"{
                "name": "Sprint 2",
                "path": "Project\\Sprint 2",
                "attributes": { "finishDate": "2026-06-04T00:00:00Z" }
            }"#,
        ),
    ]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_milestone("  Sprint   2  ", "desc")
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Created(stable_iteration_number("Project\\Sprint 2"))
    );
    let calls = runner.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0][0..4], ["boards", "iteration", "project", "list"]);
    assert_eq!(calls[1][0..4], ["boards", "iteration", "project", "create"]);
    assert!(calls[1].windows(2).any(|w| w == ["--name", "Sprint 2"]));
    assert_eq!(calls[2][0..4], ["boards", "iteration", "project", "update"]);
    assert!(
        calls[2]
            .windows(2)
            .any(|w| w == ["--path", "Project\\Sprint 2"])
    );
    assert!(calls[2].windows(2).any(|w| w == ["--description", "desc"]));
}

#[tokio::test]
async fn create_milestone_duplicate_create_error_recovers_existing_iteration() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::failure("VS403474: iteration already exists"),
        MockAzRunner::success(
            r#"[{
                "name": "Sprint 2",
                "path": "Project\\Sprint 2",
                "attributes": { "finishDate": "2026-06-04T00:00:00Z" }
            }]"#,
        ),
    ]));
    let client = test_client(runner.clone());

    let outcome = client.create_milestone("Sprint 2", "desc").await.unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: stable_iteration_number("Project\\Sprint 2"),
            state: "open".to_string()
        }
    );
    let calls = runner.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[1][0..4], ["boards", "iteration", "project", "create"]);
    assert_eq!(calls[2][0..4], ["boards", "iteration", "project", "list"]);
}
