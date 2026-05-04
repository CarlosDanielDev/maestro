use super::{MockAzRunner, test_client};
use crate::provider::github::client::{CreateOutcome, RepoProvider};
use std::sync::Arc;

fn work_item_json(id: u64, title: &str, state: &str) -> String {
    format!(
        r#"[{{
            "id": {id},
            "fields": {{
                "System.Title": "{title}",
                "System.State": "{state}"
            }},
            "url": "https://dev.azure.com/example/Project/_apis/wit/workItems/{id}"
        }}]"#
    )
}

#[tokio::test]
async fn create_issue_happy_path_returns_created_id() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(r#"{"id":123}"#),
    ]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_issue("  New   story  ", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(outcome, CreateOutcome::Created(123));
    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0..2], ["boards", "query"]);
    assert!(calls[1].windows(2).any(|w| w == ["--title", "New story"]));
    assert!(calls[1].windows(2).any(|w| w == ["--description", "Body"]));
    assert!(calls[1].windows(2).any(|w| w == ["--type", "User Story"]));
    assert!(
        calls[1]
            .windows(2)
            .any(|w| w == ["--org", "https://dev.azure.com/example"])
    );
    assert!(calls[1].windows(2).any(|w| w == ["--project", "Project"]));
}

#[tokio::test]
async fn create_issue_sends_semicolon_joined_labels() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(r#"{"id":124}"#),
    ]));
    let client = test_client(runner.clone());
    let labels = vec!["maestro:ready".to_string(), "priority:P1".to_string()];

    client
        .create_issue("Title", "Body", &labels, None)
        .await
        .unwrap();

    assert!(
        runner.calls()[1]
            .windows(2)
            .any(|w| w == ["--fields", "System.Tags=maestro:ready; priority:P1"])
    );
}

#[tokio::test]
async fn create_issue_errors_when_requested_milestone_is_missing() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success("[]"),
    ]));
    let client = test_client(runner.clone());

    let err = client
        .create_issue("Title", "Body", &[], Some(99))
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("Azure DevOps iteration for milestone id 99 not found"));
    assert_eq!(runner.calls().len(), 2);
}

#[tokio::test]
async fn create_issue_propagates_az_stderr() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::failure("VS403474: malformed field value"),
    ]));
    let client = test_client(runner);

    let err = client
        .create_issue("Title", "Body", &[], None)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("VS403474: malformed field value"));
}

#[tokio::test]
async fn create_issue_exact_duplicate_title_returns_existed_without_create() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(r#"[{"id":101}]"#),
        MockAzRunner::success(&work_item_json(101, "feat: login page", "Active")),
    ]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_issue("feat: login page", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: 101,
            state: "open".to_string()
        }
    );
    assert_eq!(runner.calls().len(), 2);
    assert!(!runner.calls().iter().flatten().any(|arg| arg == "create"));
}

#[tokio::test]
async fn create_issue_case_difference_duplicate_title_returns_existed() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(r#"[{"id":102}]"#),
        MockAzRunner::success(&work_item_json(102, "Feat: Login Page", "Active")),
    ]));
    let client = test_client(runner);

    let outcome = client
        .create_issue("feat: login page", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(outcome.number(), 102);
    assert!(outcome.is_existed());
}

#[tokio::test]
async fn create_issue_whitespace_difference_duplicate_title_returns_existed() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(r#"[{"id":103}]"#),
        MockAzRunner::success(&work_item_json(103, "feat: login page", "Active")),
    ]));
    let client = test_client(runner);

    let outcome = client
        .create_issue("  feat:   login   page  ", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(outcome.number(), 103);
    assert!(outcome.is_existed());
}

#[tokio::test]
async fn create_issue_no_duplicate_match_creates_new_work_item() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(r#"[{"id":104}]"#),
        MockAzRunner::success(&work_item_json(104, "feat: other", "Active")),
        MockAzRunner::success(r#"{"id":105}"#),
    ]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_issue("feat: login page", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(outcome, CreateOutcome::Created(105));
    let calls = runner.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2][0..3], ["boards", "work-item", "create"]);
}

#[tokio::test]
async fn create_issue_duplicate_create_error_recovers_existing_work_item() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::failure("TF401289: work item already exists"),
        MockAzRunner::success(r#"[{"id":106}]"#),
        MockAzRunner::success(&work_item_json(106, "feat: login page", "Active")),
    ]));
    let client = test_client(runner.clone());

    let outcome = client
        .create_issue("feat: login page", "Body", &[], None)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: 106,
            state: "open".to_string()
        }
    );
    assert_eq!(runner.calls().len(), 4);
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
