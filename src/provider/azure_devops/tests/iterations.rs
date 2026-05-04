use super::{MockAzRunner, test_client};
use crate::provider::azure_devops::iterations::{
    filter_iterations_by_state, iteration_path_for_milestone_number, iterations_to_milestones,
    parse_iterations_json, stable_iteration_number,
};
use crate::provider::github::client::RepoProvider;
use chrono::NaiveDate;
use std::sync::Arc;

pub(super) fn iterations_fixture() -> &'static str {
    r#"[
        {
            "identifier": "11111111-1111-1111-1111-111111111111",
            "name": "Backlog",
            "path": "Project\\Backlog",
            "structureType": "iteration",
            "attributes": {},
            "url": "https://dev.azure.com/example/Project/_apis/wit/classificationNodes/Iterations/Backlog"
        },
        {
            "identifier": "22222222-2222-2222-2222-222222222222",
            "name": "Sprint Past",
            "path": "Project\\Sprint Past",
            "structureType": "iteration",
            "attributes": { "finishDate": "2026-05-03T00:00:00Z" },
            "url": "https://dev.azure.com/example/Project/_apis/wit/classificationNodes/Iterations/Sprint%20Past"
        },
        {
            "identifier": "33333333-3333-3333-3333-333333333333",
            "name": "Sprint Future",
            "path": "Project\\Sprint Future",
            "structureType": "iteration",
            "attributes": { "finishDate": "2026-06-04T00:00:00Z" },
            "url": "https://dev.azure.com/example/Project/_apis/wit/classificationNodes/Iterations/Sprint%20Future"
        }
    ]"#
}

fn work_item_details() -> &'static str {
    r#"[{
        "id": 10,
        "fields": {
            "System.Title": "Scoped story",
            "System.State": "Active"
        },
        "url": "https://dev.azure.com/example/Project/_apis/wit/workItems/10"
    }]"#
}

#[test]
fn parse_iterations_json_maps_nodes_to_milestones() {
    let iterations = parse_iterations_json(iterations_fixture()).unwrap();

    assert_eq!(iterations.len(), 3);
    assert_eq!(iterations[2].title, "Sprint Future");
    assert_eq!(iterations[2].path, "Project\\Sprint Future");
    assert_eq!(
        iterations[2].finish_date,
        Some(NaiveDate::from_ymd_opt(2026, 6, 4).unwrap())
    );

    let milestones =
        iterations_to_milestones(iterations, NaiveDate::from_ymd_opt(2026, 5, 4).unwrap());
    assert_eq!(milestones[0].title, "Backlog");
    assert_eq!(milestones[0].description, "");
    assert_eq!(milestones[0].state, "open");
    assert_eq!(milestones[0].open_issues, 0);
    assert_eq!(milestones[0].closed_issues, 0);
    assert_eq!(
        milestones[2].number,
        stable_iteration_number("Project\\Sprint Future")
    );
}

#[test]
fn parse_iterations_json_flattens_children() {
    let json = r#"[{
        "name": "Parent",
        "path": "Project\\Parent",
        "attributes": {},
        "children": [{
            "name": "Child",
            "path": "Project\\Parent\\Child",
            "attributes": { "finishDate": "2026-06-04T00:00:00Z" }
        }]
    }]"#;

    let iterations = parse_iterations_json(json).unwrap();

    assert_eq!(iterations.len(), 2);
    assert_eq!(iterations[1].title, "Child");
    assert_eq!(iterations[1].path, "Project\\Parent\\Child");
}

#[test]
fn filter_iterations_by_state_open_keeps_future_and_unset_finish_dates() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
    let iterations = parse_iterations_json(iterations_fixture()).unwrap();

    let filtered = filter_iterations_by_state(iterations, "open", today).unwrap();

    assert_eq!(
        filtered
            .iter()
            .map(|iteration| iteration.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Backlog", "Sprint Future"]
    );
}

#[test]
fn filter_iterations_by_state_closed_keeps_past_finish_dates() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
    let iterations = parse_iterations_json(iterations_fixture()).unwrap();

    let filtered = filter_iterations_by_state(iterations, "closed", today).unwrap();

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].title, "Sprint Past");
}

#[test]
fn filter_iterations_by_state_all_bypasses_filter() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
    let iterations = parse_iterations_json(iterations_fixture()).unwrap();

    let filtered = filter_iterations_by_state(iterations, "all", today).unwrap();

    assert_eq!(filtered.len(), 3);
}

#[test]
fn parse_iterations_json_empty_array() {
    let iterations = parse_iterations_json("[]").unwrap();

    assert!(iterations.is_empty());
}

#[test]
fn parse_iterations_json_malformed_az_output_returns_err() {
    let err = parse_iterations_json("{not json}").unwrap_err().to_string();

    assert!(err.contains("Failed to parse Azure DevOps iterations JSON"));
}

#[tokio::test]
async fn create_issue_resolves_milestone_number_to_iteration_path() {
    let iteration_number = stable_iteration_number("Project\\Sprint Future");
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(iterations_fixture()),
        MockAzRunner::success(r#"{"id":126}"#),
    ]));
    let client = test_client(runner.clone());

    client
        .create_issue("Title", "Body", &[], Some(iteration_number))
        .await
        .unwrap();

    let calls = runner.calls();
    assert_eq!(calls.len(), 3);
    assert!(
        calls[1]
            .windows(4)
            .any(|w| { w == ["boards", "iteration", "project", "list"] })
    );
    assert!(
        calls[2]
            .windows(2)
            .any(|w| w == ["--iteration", "Project\\Sprint Future"])
    );
}

#[tokio::test]
async fn list_issues_by_milestone_resolves_iteration_title_to_path() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(iterations_fixture()),
        MockAzRunner::success(r#"[{"id":10}]"#),
        MockAzRunner::success(work_item_details()),
    ]));
    let client = test_client(runner.clone());

    let issues = client
        .list_issues_by_milestone("Sprint Future")
        .await
        .unwrap();

    assert_eq!(issues.len(), 1);
    let calls = runner.calls();
    assert_eq!(calls.len(), 3);
    assert!(
        calls[1]
            .windows(2)
            .any(|w| w == ["--wiql", "SELECT [System.Id] FROM WorkItems WHERE [System.IterationPath] UNDER 'Project\\Sprint Future' AND [System.State] <> 'Closed' AND [System.State] <> 'Removed'"])
    );
}

#[tokio::test]
async fn list_issues_by_milestone_escapes_iteration_path_for_wiql() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(
            r#"[{
                "name": "Bob's Sprint",
                "path": "Project\\Bob's Sprint",
                "attributes": {}
            }]"#,
        ),
        MockAzRunner::success("[]"),
    ]));
    let client = test_client(runner.clone());

    let issues = client
        .list_issues_by_milestone("Bob's Sprint")
        .await
        .unwrap();

    assert!(issues.is_empty());
    assert!(
        runner.calls()[1]
            .windows(2)
            .any(|w| w == ["--wiql", "SELECT [System.Id] FROM WorkItems WHERE [System.IterationPath] UNDER 'Project\\Bob''s Sprint' AND [System.State] <> 'Closed' AND [System.State] <> 'Removed'"])
    );
}

#[tokio::test]
async fn list_milestones_all_calls_az_and_returns_all_iterations() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        iterations_fixture(),
    )]));
    let client = test_client(runner.clone());

    let milestones = client.list_milestones("all").await.unwrap();

    assert_eq!(milestones.len(), 3);
    assert_eq!(milestones[0].title, "Backlog");
    assert_eq!(milestones[1].state, "closed");
    assert_eq!(milestones[2].state, "open");
    assert_eq!(
        runner.calls()[0],
        vec![
            "boards",
            "iteration",
            "project",
            "list",
            "--org",
            "https://dev.azure.com/example",
            "--project",
            "Project",
            "-o",
            "json",
        ]
    );
}

#[tokio::test]
async fn list_milestones_open_filters_past_iterations() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        iterations_fixture(),
    )]));
    let client = test_client(runner);

    let milestones = client.list_milestones("open").await.unwrap();

    assert_eq!(
        milestones
            .iter()
            .map(|milestone| milestone.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Backlog", "Sprint Future"]
    );
}

#[tokio::test]
async fn list_milestones_closed_filters_future_and_unset_iterations() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        iterations_fixture(),
    )]));
    let client = test_client(runner);

    let milestones = client.list_milestones("closed").await.unwrap();

    assert_eq!(milestones.len(), 1);
    assert_eq!(milestones[0].title, "Sprint Past");
}

#[tokio::test]
async fn list_milestones_empty_list_returns_empty() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("[]")]));
    let client = test_client(runner);

    let milestones = client.list_milestones("all").await.unwrap();

    assert!(milestones.is_empty());
}

#[tokio::test]
async fn list_milestones_malformed_az_output_returns_err() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("{not json}")]));
    let client = test_client(runner);

    let err = client.list_milestones("all").await.unwrap_err().to_string();

    assert!(err.contains("Failed to parse Azure DevOps iterations JSON"));
}

#[test]
fn iteration_path_for_milestone_number_round_trips_listed_hash() {
    let iterations = parse_iterations_json(iterations_fixture()).unwrap();
    let number = stable_iteration_number("Project\\Sprint Future");

    assert_eq!(
        iteration_path_for_milestone_number(&iterations, number).as_deref(),
        Some("Project\\Sprint Future")
    );
}
