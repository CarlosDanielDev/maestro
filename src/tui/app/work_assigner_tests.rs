use super::*;
use crate::config::Config;
use crate::models::ModelRouter;
use crate::provider::types::Issue;
use crate::work::assigner::WorkAssigner;
use crate::work::types::{WorkItem, WorkStatus};
use std::collections::HashMap;

fn make_config() -> Config {
    toml::from_str(
        r#"
        [project]
        repo = "owner/repo"
        base_branch = "main"
        [sessions]
        [budget]
        [github]
        [notifications]
        "#,
    )
    .unwrap()
}

fn make_config_with_heavy(labels: &[&str], limit: usize) -> Config {
    let labels_toml = labels
        .iter()
        .map(|l| format!(r#""{}""#, l))
        .collect::<Vec<_>>()
        .join(", ");
    toml::from_str(&format!(
        r#"
        [project]
        repo = "owner/repo"
        base_branch = "main"
        [sessions]
        [budget]
        [github]
        [notifications]
        [concurrency]
        heavy_task_labels = [{}]
        heavy_task_limit = {}
        "#,
        labels_toml, limit
    ))
    .unwrap()
}

fn make_work_item(number: u64, labels: &[&str]) -> WorkItem {
    WorkItem::from_issue(Issue {
        number,
        title: format!("Issue #{}", number),
        body: String::new(),
        labels: labels.iter().map(|s| s.to_string()).collect(),
        state: "open".to_string(),
        html_url: String::new(),
        milestone: None,
        assignees: vec![],
    })
}

#[test]
fn assign_work_returns_empty_when_no_slots_available() {
    let items = vec![make_work_item(1, &[])];
    let assigner = WorkAssigner::new(items);
    let mut service = WorkAssignmentService::new(assigner);
    let config = make_config();
    let ctx = AssignmentContext {
        available_slots: 0,
        config: &config,
        model_router: None,
    };

    let assignments = service.assign_work(&ctx);

    assert!(assignments.is_empty());
    assert_eq!(
        service.inner().all_items()[0].status,
        WorkStatus::Pending,
        "mark_in_progress must not be called when slots == 0"
    );
}

#[test]
fn assign_work_returns_assignment_when_slot_available() {
    let items = vec![make_work_item(42, &[])];
    let assigner = WorkAssigner::new(items);
    let mut service = WorkAssignmentService::new(assigner);
    let config = make_config();
    let ctx = AssignmentContext {
        available_slots: 1,
        config: &config,
        model_router: None,
    };

    let assignments = service.assign_work(&ctx);

    assert_eq!(assignments.len(), 1);
    let a = &assignments[0];
    assert_eq!(a.issue_number, 42);
    assert_eq!(a.title, "Issue #42");
    assert!(!a.prompt.is_empty(), "prompt must not be empty");
    assert_eq!(a.model, config.sessions.default_model);
    assert_eq!(a.mode, config.sessions.default_mode);
    assert_eq!(
        service.inner().all_items()[0].status,
        WorkStatus::InProgress
    );
}

#[test]
fn assign_work_does_not_exceed_heavy_task_limit() {
    let items = vec![
        make_work_item(10, &["heavy", "priority:P0"]),
        make_work_item(11, &["heavy", "priority:P0"]),
    ];
    let assigner = WorkAssigner::new(items);
    let mut service = WorkAssignmentService::new(assigner);
    let config = make_config_with_heavy(&["heavy"], 1);
    let ctx = AssignmentContext {
        available_slots: 2,
        config: &config,
        model_router: None,
    };

    let assignments = service.assign_work(&ctx);

    assert_eq!(assignments.len(), 1, "heavy limit of 1 must be respected");
    assert_eq!(assignments[0].issue_number, 10);
    let item_11 = service
        .inner()
        .all_items()
        .iter()
        .find(|i| i.number() == 11)
        .unwrap();
    assert_eq!(item_11.status, WorkStatus::Pending);
}

#[test]
fn assign_work_uses_model_router_when_present() {
    let items = vec![make_work_item(5, &["priority:P0"])];
    let assigner = WorkAssigner::new(items);
    let mut service = WorkAssignmentService::new(assigner);
    let config = make_config();
    let mut rules = HashMap::new();
    rules.insert("priority:P0".to_string(), "claude-opus-4".to_string());
    let router = ModelRouter::new(rules, config.sessions.default_model.clone());
    let ctx = AssignmentContext {
        available_slots: 1,
        config: &config,
        model_router: Some(&router),
    };

    let assignments = service.assign_work(&ctx);

    assert_eq!(assignments.len(), 1);
    assert_eq!(
        assignments[0].model, "claude-opus-4",
        "router must override config default when a rule matches"
    );
}

#[test]
fn assign_work_uses_item_mode_over_config_default() {
    let items = vec![make_work_item(7, &["mode:vibe"])];
    let assigner = WorkAssigner::new(items);
    let mut service = WorkAssignmentService::new(assigner);
    let config = make_config();
    let ctx = AssignmentContext {
        available_slots: 1,
        config: &config,
        model_router: None,
    };

    let assignments = service.assign_work(&ctx);

    assert_eq!(assignments.len(), 1);
    assert_eq!(
        assignments[0].mode, "vibe",
        "item mode label must override config default_mode"
    );
}

#[test]
fn assign_work_respects_available_slots_count() {
    let items = vec![
        make_work_item(1, &[]),
        make_work_item(2, &[]),
        make_work_item(3, &[]),
    ];
    let assigner = WorkAssigner::new(items);
    let mut service = WorkAssignmentService::new(assigner);
    let config = make_config();
    let ctx = AssignmentContext {
        available_slots: 1,
        config: &config,
        model_router: None,
    };

    let assignments = service.assign_work(&ctx);

    assert_eq!(assignments.len(), 1);
}

// --- #1140: `run --issue` start-time label transition ---

fn make_app_with_mock(mock: crate::provider::github::client::mock::MockGitHubClient) -> App {
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    let tmp = std::env::temp_dir().join(format!(
        "maestro-work-assigner-test-{}.json",
        uuid::Uuid::new_v4()
    ));
    let store = StateStore::new(tmp);
    let mut app = App::new(
        store,
        3,
        Box::new(MockWorktreeManager::new()),
        "bypassPermissions".into(),
        vec![],
    );
    app.gh_auth_ok = true;
    app.config = Some(make_config());
    app.github_client = Some(std::sync::Arc::new(mock));
    app
}

#[tokio::test]
async fn run_issue_start_marks_in_progress_and_clears_ready() {
    use crate::provider::github::client::mock::MockGitHubClient;

    let mock = MockGitHubClient::new();
    let client = mock.clone();
    let mut app = make_app_with_mock(mock);

    app.mark_issues_in_progress(&client, &[41, 42]).await;

    let adds = client.add_label_calls();
    let removes = client.remove_label_calls();
    for num in [41, 42] {
        assert!(
            adds.iter()
                .any(|(n, l)| *n == num && l == "maestro:in-progress"),
            "expected maestro:in-progress on #{num}"
        );
        assert!(
            removes
                .iter()
                .any(|(n, l)| *n == num && l == "maestro:ready"),
            "expected maestro:ready removed from #{num}"
        );
    }
}

#[tokio::test]
async fn run_issue_start_label_failure_is_logged_not_fatal() {
    use crate::provider::github::client::mock::MockGitHubClient;
    use crate::tui::activity_log::LogLevel;

    let mock = MockGitHubClient::new();
    mock.set_add_label_error("TLS handshake timeout");
    let client = mock.clone();
    let mut app = make_app_with_mock(mock);

    app.mark_issues_in_progress(&client, &[41]).await;

    assert!(
        app.activity_log
            .entries()
            .iter()
            .any(|e| e.level == LogLevel::Error && e.message.contains("Label update failed")),
        "expected a Label update failed entry in the activity log"
    );
}

#[tokio::test]
async fn run_issue_start_skips_labels_when_gh_auth_lost() {
    use crate::provider::github::client::mock::MockGitHubClient;

    let mock = MockGitHubClient::new();
    let client = mock.clone();
    let mut app = make_app_with_mock(mock);
    app.gh_auth_ok = false;

    app.mark_issues_in_progress(&client, &[41]).await;

    assert_eq!(
        client.add_label_attempts(),
        0,
        "no label calls expected without auth"
    );
}
