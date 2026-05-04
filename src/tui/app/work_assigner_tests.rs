use super::*;
use crate::config::Config;
use crate::models::ModelRouter;
use crate::provider::github::types::GhIssue;
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
    WorkItem::from_issue(GhIssue {
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
