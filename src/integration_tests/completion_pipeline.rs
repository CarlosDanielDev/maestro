use crate::integration_tests::helpers::*;
use crate::provider::github::client::GitHubClient;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::github::labels::LabelManager;
use crate::provider::github::pr::{PrCreator, build_pr_body};
use crate::session::manager::ManagedSession;
use crate::session::types::StreamEvent;

#[tokio::test]
async fn mark_done_transitions_labels_correctly() {
    let client = MockGitHubClient::new();
    let mgr = LabelManager::new(client.clone());

    mgr.mark_done(42).await.unwrap();

    let adds = client.add_label_calls();
    let removes = client.remove_label_calls();

    assert!(adds.iter().any(|(n, l)| *n == 42 && l == "maestro:done"));
    assert!(
        removes
            .iter()
            .any(|(n, l)| *n == 42 && l == "maestro:in-progress")
    );
}

#[tokio::test]
async fn mark_done_adds_label_before_removing() {
    let client = MockGitHubClient::new();
    let mgr = LabelManager::new(client.clone());

    mgr.mark_done(7).await.unwrap();

    let adds = client.add_label_calls();
    let removes = client.remove_label_calls();

    assert_eq!(adds.len(), 1);
    assert_eq!(removes.len(), 1);
    assert_eq!(adds[0].1, "maestro:done");
    assert_eq!(removes[0].1, "maestro:in-progress");
}

#[tokio::test]
async fn pr_created_with_correct_metadata() {
    let client = MockGitHubClient::new();
    client.set_create_pr_response(55);
    let creator = PrCreator::new(client.clone(), "main".to_string());
    let issue = make_gh_issue(42);

    let pr_number = creator
        .create_for_issue(&issue, "maestro/issue-42", &["src/session/pool.rs"], 0.88)
        .await
        .unwrap();

    assert_eq!(pr_number, 55);

    let calls = client.create_pr_calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.issue_number, 42);
    assert_eq!(call.head_branch, "maestro/issue-42");
    assert_eq!(call.base_branch, "main");
    assert!(call.body.contains("Closes #42"));
    assert!(call.body.contains("src/session/pool.rs"));
    assert!(call.title.contains("42"));
}

#[tokio::test]
async fn full_pipeline_label_and_pr_for_completed_session() {
    let client = MockGitHubClient::new();
    client.set_issues(vec![make_gh_issue(15)]);
    client.set_create_pr_response(99);

    let label_mgr = LabelManager::new(client.clone());
    let pr_creator = PrCreator::new(client.clone(), "main".to_string());

    let mut managed = ManagedSession::new(make_running_session_with_issue(15));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Write".to_string(),

        file_path: Some("src/github/pr.rs".to_string()),
        command_preview: None,
        subagent_name: None,
    });
    managed.handle_event(&StreamEvent::Completed { cost_usd: 0.77 });

    assert_eq!(
        managed.session.status,
        crate::session::types::SessionStatus::Completed
    );

    label_mgr.mark_done(15).await.unwrap();

    let issue = client.get_issue(15).await.unwrap();
    let files_ref: Vec<&str> = managed
        .session
        .files_touched
        .iter()
        .map(|s| s.as_str())
        .collect();
    let pr_number = pr_creator
        .create_for_issue(
            &issue,
            "maestro/issue-15",
            &files_ref,
            managed.session.cost_usd,
        )
        .await
        .unwrap();

    assert_eq!(pr_number, 99);
    let pr_calls = client.create_pr_calls();
    assert!(pr_calls[0].body.contains("0.77") || pr_calls[0].body.contains("$0.77"));
}

#[tokio::test]
async fn error_session_marks_failed_not_done() {
    let client = MockGitHubClient::new();
    let label_mgr = LabelManager::new(client.clone());
    let mut managed = ManagedSession::new(make_running_session_with_issue(20));

    managed.handle_event(&StreamEvent::Error {
        message: "process died".to_string(),
    });
    assert_eq!(
        managed.session.status,
        crate::session::types::SessionStatus::Errored
    );

    label_mgr.mark_failed(20).await.unwrap();

    let adds = client.add_label_calls();
    assert!(adds.iter().any(|(n, l)| *n == 20 && l == "maestro:failed"));
    assert_eq!(
        adds.iter().filter(|(_, l)| l == "maestro:done").count(),
        0,
        "maestro:done must NOT be added for an errored session"
    );
}

// ---------------------------------------------------------------------------
// PR body content tests
// ---------------------------------------------------------------------------

#[test]
fn pr_body_closes_reference_uses_correct_issue_number() {
    let issue = make_gh_issue(42);
    let body = build_pr_body(&issue, &[], 0.0);
    assert!(body.contains("Closes #42"));
}

#[test]
fn pr_body_includes_all_touched_files() {
    let issue = make_gh_issue(1);
    let files = vec!["src/session/pool.rs", "src/github/labels.rs", "src/main.rs"];
    let body = build_pr_body(&issue, &files, 0.0);
    for file in &files {
        assert!(body.contains(file), "PR body must mention file: {}", file);
    }
}

#[test]
fn pr_body_includes_cost_in_dollars() {
    let issue = make_gh_issue(1);
    let body = build_pr_body(&issue, &[], 2.75);
    assert!(body.contains("2.75"));
}

#[test]
fn pr_body_empty_files_does_not_panic() {
    let issue = make_gh_issue(1);
    let body = build_pr_body(&issue, &[], 0.0);
    assert!(body.contains("Closes #1"));
}
