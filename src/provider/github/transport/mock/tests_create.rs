use super::*;
use crate::provider::github::transport::GitHubClient;
use crate::provider::github::types::{GhIssue, GhMilestone};

// -- Issue #453: CreateOutcome proactive pre-check --

fn gh_issue(number: u64, title: &str, state: &str) -> GhIssue {
    GhIssue {
        number,
        title: title.to_string(),
        body: String::new(),
        labels: Vec::new(),
        state: state.to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: None,
        assignees: Vec::new(),
    }
}

fn gh_milestone(number: u64, title: &str, state: &str) -> GhMilestone {
    GhMilestone {
        number,
        title: title.to_string(),
        description: String::new(),
        state: state.to_string(),
        open_issues: 0,
        closed_issues: 0,
    }
}

#[tokio::test]
async fn create_milestone_proactive_hit_returns_existed_without_call() {
    let client = MockGitHubClient::new();
    client.set_existing_milestones(vec![gh_milestone(42, "M0: Foundation", "open")]);

    let outcome = client
        .create_milestone("M0: Foundation", "desc")
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: 42,
            state: "open".into()
        }
    );
    assert!(
        client.create_milestone_calls().is_empty(),
        "no POST should have been recorded"
    );
}

#[tokio::test]
async fn create_milestone_proactive_hit_tolerates_whitespace_and_case() {
    let client = MockGitHubClient::new();
    client.set_existing_milestones(vec![gh_milestone(42, "M0: Foundation", "open")]);

    let outcome = client
        .create_milestone("  m0:   foundation  ", "desc")
        .await
        .unwrap();

    assert!(matches!(outcome, CreateOutcome::Existed { number: 42, .. }));
    assert!(client.create_milestone_calls().is_empty());
}

#[tokio::test]
async fn create_milestone_finds_closed_milestone_and_returns_state() {
    let client = MockGitHubClient::new();
    client.set_existing_milestones(vec![gh_milestone(9, "M0: Done", "closed")]);

    let outcome = client.create_milestone("M0: Done", "desc").await.unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: 9,
            state: "closed".into()
        }
    );
}

#[tokio::test]
async fn create_milestone_creates_new_when_no_match() {
    let client = MockGitHubClient::new();
    client.set_existing_milestones(vec![gh_milestone(1, "M0", "open")]);

    let outcome = client.create_milestone("M1", "desc").await.unwrap();

    assert!(matches!(outcome, CreateOutcome::Created(_)));
    assert_eq!(client.create_milestone_calls().len(), 1);
}

#[tokio::test]
async fn create_issue_proactive_hit_returns_existed_without_call() {
    let client = MockGitHubClient::new();
    client.set_existing_issues(vec![gh_issue(100, "feat: login page", "open")]);

    let outcome = client
        .create_issue("feat: login page", "body", &[], None)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: 100,
            state: "open".into()
        }
    );
    assert!(
        client.create_issue_calls().is_empty(),
        "no POST should have been recorded"
    );
}

#[tokio::test]
async fn create_issue_finds_closed_issue() {
    let client = MockGitHubClient::new();
    client.set_existing_issues(vec![gh_issue(77, "feat: done", "closed")]);

    let outcome = client
        .create_issue("feat: done", "body", &[], None)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        CreateOutcome::Existed {
            number: 77,
            state: "closed".into()
        }
    );
}

#[tokio::test]
async fn create_issue_rejects_empty_title_before_list_call() {
    let client = MockGitHubClient::new();
    let result = client.create_issue("", "body", &[], None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_issue_rejects_whitespace_only_title() {
    let client = MockGitHubClient::new();
    let result = client.create_issue("   ", "body", &[], None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_issue_rejects_leading_dash_title() {
    let client = MockGitHubClient::new();
    let result = client.create_issue("-evil", "body", &[], None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_issue_rejects_oversize_title() {
    let client = MockGitHubClient::new();
    let huge = "x".repeat(300);
    let result = client.create_issue(&huge, "body", &[], None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_milestone_rejects_empty_title() {
    let client = MockGitHubClient::new();
    let result = client.create_milestone("", "desc").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_outcome_number_helper() {
    assert_eq!(CreateOutcome::Created(42).number(), 42);
    assert_eq!(
        CreateOutcome::Existed {
            number: 7,
            state: "open".into(),
        }
        .number(),
        7
    );
}
