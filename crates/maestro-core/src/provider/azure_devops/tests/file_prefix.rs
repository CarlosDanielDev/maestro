//! az expands `@file` in every argv element (azure-cli 2.90.0,
//! `azure/cli/core/commands/__init__.py`, `_expand_file_prefixed_files`):
//! it splits the element on its first `=` and reads the rest as a file when
//! that rest starts with `@`. A separate value `Sprint=@/etc/passwd` is
//! therefore read as a file; `--name=Sprint=@/etc/passwd` is not, because the
//! split leaves `Sprint=@/etc/passwd`. These tests pin the exact argv.

use super::{MockAzRunner, test_client};
use crate::provider::azure_devops::iterations::stable_iteration_number;
use crate::provider::github::client::RepoProvider;
use std::sync::Arc;

const ORG: &str = "https://dev.azure.com/example";

fn iteration_json(name: &str, path: &str) -> String {
    serde_json::json!({
        "name": name,
        "path": path,
        "attributes": { "finishDate": "2999-12-31T00:00:00Z" }
    })
    .to_string()
}

fn work_item_with_tags(tags: &str) -> String {
    serde_json::json!({
        "id": 1,
        "fields": { "System.Title": "T", "System.State": "Active", "System.Tags": tags },
        "url": "https://dev.azure.com/example/Project/_apis/wit/workItems/1"
    })
    .to_string()
}

fn assert_refused(err: anyhow::Error) {
    assert!(err.to_string().contains("'@'"), "got: {err}");
}

#[tokio::test]
async fn create_milestone_binds_name_and_path_so_equals_at_is_not_a_file() {
    let title = "Sprint=@/etc/passwd";
    let path = "Project\\Sprint=@/etc/passwd";
    let created = iteration_json(title, path);
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(&created),
        MockAzRunner::success(&created),
    ]));
    let client = test_client(runner.clone());

    client.create_milestone(title, "desc").await.unwrap();

    let calls = runner.calls();
    assert_eq!(
        calls[1],
        [
            "boards",
            "iteration",
            "project",
            "create",
            "--name=Sprint=@/etc/passwd",
            "--org",
            ORG,
            "--project",
            "Project",
            "-o",
            "json"
        ]
    );
    assert_eq!(
        calls[2],
        [
            "boards",
            "iteration",
            "project",
            "update",
            "--path=Project\\Sprint=@/etc/passwd",
            "--description=desc",
            "--org",
            ORG,
            "--project",
            "Project",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn create_issue_binds_title_and_iteration_so_equals_at_is_not_a_file() {
    let path = "Project\\Sprint=@/etc/passwd";
    let iterations = format!("[{}]", iteration_json("Sprint=@/etc/passwd", path));
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success("[]"),
        MockAzRunner::success(&iterations),
        MockAzRunner::success(r#"{"id":7}"#),
    ]));
    let client = test_client(runner.clone());

    let milestone = stable_iteration_number(path);
    client
        .create_issue("Story=@/etc/passwd", "Body", &[], Some(milestone))
        .await
        .unwrap();

    assert_eq!(
        runner.calls()[2],
        [
            "boards",
            "work-item",
            "create",
            "--type",
            "User Story",
            "--title=Story=@/etc/passwd",
            "--description=Body",
            "--org",
            ORG,
            "--project",
            "Project",
            "--iteration=Project\\Sprint=@/etc/passwd",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn create_pr_binds_title_and_branches_so_equals_at_is_not_a_file() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        r#"{"pullRequestId":5}"#,
    )]));
    let client = test_client(runner.clone());

    client
        .create_pr(1, "T=@/etc/passwd", "body", "feat=@/etc/passwd", "main")
        .await
        .unwrap();

    assert_eq!(
        runner.calls()[0],
        [
            "repos",
            "pr",
            "create",
            "--title=T=@/etc/passwd",
            "--description=body",
            "--source-branch=feat=@/etc/passwd",
            "--target-branch=main",
            "--org",
            ORG,
            "--project",
            "Project",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn list_prs_for_branch_binds_branch_so_equals_at_is_not_a_file() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("[]")]));
    let client = test_client(runner.clone());

    client
        .list_prs_for_branch("feat=@/etc/passwd")
        .await
        .unwrap();

    assert_eq!(
        runner.calls()[0],
        [
            "repos",
            "pr",
            "list",
            "--source-branch=feat=@/etc/passwd",
            "--status",
            "active",
            "--org",
            ORG,
            "--project",
            "Project",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn list_issues_binds_wiql_so_equals_at_tag_is_not_a_file() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success("[]")]));
    let client = test_client(runner.clone());

    client.list_issues(&["x=@/etc/passwd"]).await.unwrap();

    assert_eq!(
        runner.calls()[0],
        [
            "boards",
            "query",
            "--wiql=SELECT [System.Id] FROM WorkItems WHERE [System.State] <> 'Closed' \
             AND [System.State] <> 'Removed' AND [System.Tags] CONTAINS 'x=@/etc/passwd'",
            "--org",
            ORG,
            "--project",
            "Project",
            "-o",
            "json"
        ]
    );
}

#[tokio::test]
async fn ci_check_runs_for_pr_binds_head_branch_so_equals_at_is_not_a_file() {
    let runner = Arc::new(MockAzRunner::new(vec![
        MockAzRunner::success(
            r#"{"pullRequestId":44,"sourceRefName":"refs/heads/b=@/etc/passwd","status":"active"}"#,
        ),
        MockAzRunner::success("[]"),
    ]));
    let client = test_client(runner.clone());

    client.ci_check_runs_for_pr(44).await.unwrap();

    let calls = runner.calls();
    assert!(
        calls[1].contains(&"--branch=refs/heads/b=@/etc/passwd".to_string()),
        "got: {:?}",
        calls[1]
    );
}

// `System.Tags=@x` splits into `System.Tags` + `@x`, and az reads `@x`.
#[tokio::test]
async fn create_issue_rejects_labels_starting_with_at() {
    let runner = Arc::new(MockAzRunner::new(vec![]));
    let client = test_client(runner.clone());

    let labels = vec!["@/etc/passwd".to_string()];
    let err = client
        .create_issue("Story", "Body", &labels, None)
        .await
        .expect_err("@-prefixed labels");
    assert_refused(err);
    assert!(runner.calls().is_empty(), "az must not run");
}

#[tokio::test]
async fn add_label_rejects_tags_starting_with_at_before_update() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        &work_item_with_tags(""),
    )]));
    let client = test_client(runner.clone());

    let err = client
        .add_label(1, "@/etc/passwd")
        .await
        .expect_err("@-prefixed tags");
    assert_refused(err);
    assert_eq!(runner.calls().len(), 1, "only the show call may run");
}

#[tokio::test]
async fn remove_label_rejects_tags_starting_with_at_before_update() {
    let runner = Arc::new(MockAzRunner::new(vec![MockAzRunner::success(
        &work_item_with_tags("@/etc/passwd; x"),
    )]));
    let client = test_client(runner.clone());

    let err = client
        .remove_label(1, "x")
        .await
        .expect_err("@-prefixed tags");
    assert_refused(err);
    assert_eq!(runner.calls().len(), 1, "only the show call may run");
}
