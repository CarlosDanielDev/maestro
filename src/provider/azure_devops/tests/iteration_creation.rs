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
            "attributes": { "finishDate": "2999-12-31T00:00:00Z" }
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
                "attributes": { "finishDate": "2999-12-31T00:00:00Z" }
            }"#,
        ),
        MockAzRunner::success(
            r#"{
                "name": "Sprint 2",
                "path": "Project\\Sprint 2",
                "attributes": { "finishDate": "2999-12-31T00:00:00Z" }
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
    assert!(calls[2].contains(&"--description=desc".to_string()));
}

#[tokio::test]
async fn create_milestone_binds_dash_leading_description_to_flag() {
    let iteration = r#"{
        "name": "Sprint 2",
        "path": "Project\\Sprint 2",
        "attributes": { "finishDate": "2999-12-31T00:00:00Z" }
    }"#;
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(iteration),
        MockAzRunner::success(iteration),
    ]));
    let client = test_client(runner.clone());

    let description = "- bullet list\n--org evil";
    client
        .create_milestone("Sprint 2", description)
        .await
        .unwrap();

    let update = &runner.calls()[2];
    assert!(
        update.contains(&format!("--description={description}")),
        "description must be bound to its flag, got: {update:?}"
    );
    assert!(
        !update.iter().any(|arg| arg == description),
        "got: {update:?}"
    );
}

// az expands a value starting with `@` as a file to read and upload.
#[tokio::test]
async fn create_milestone_rejects_at_prefixed_description() {
    let runner = Arc::new(MockAzRunner::new(vec![]));
    let client = test_client(runner.clone());

    let err = client
        .create_milestone("Sprint 2", "@/Users/x/.ssh/id_rsa")
        .await
        .expect_err("@-prefixed description");
    assert!(err.to_string().contains("'@'"), "got: {err}");
    assert!(runner.calls().is_empty(), "az must not run");
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
                "attributes": { "finishDate": "2999-12-31T00:00:00Z" }
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
