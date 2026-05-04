use crate::provider::github::client::{CreateOutcome, RepoProvider};
use crate::provider::types::{
    CheckRun, CiStatus, Issue, MergeMethod, Milestone, PullRequest, ReviewEvent,
};
use crate::util::{titles_equivalent, validate_body, validate_title};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;

mod issues;
mod iterations;

use issues::{build_create_work_item_args, parse_created_work_item_id};
use iterations::{
    filter_iterations_by_state, iteration_path_for_milestone_number, iteration_state,
    iterations_to_milestones, parse_iteration_json, parse_iterations_json, today_utc,
};

#[cfg(test)]
mod tests;

/// Azure DevOps client using `az` CLI.
pub struct AzDevOpsClient {
    organization: String,
    project: String,
    runner: Arc<dyn AzRunner>,
}

impl AzDevOpsClient {
    pub fn new(organization: String, project: String) -> Self {
        Self {
            organization,
            project,
            runner: Arc::new(AzCliRunner),
        }
    }

    #[cfg(test)]
    fn with_runner(organization: String, project: String, runner: Arc<dyn AzRunner>) -> Self {
        Self {
            organization,
            project,
            runner,
        }
    }

    async fn run_az(&self, args: &[&str]) -> Result<String> {
        let output =
            self.runner.run(args).await.context(
                "Failed to run `az` CLI. Is it installed? Run `az login` to authenticate.",
            )?;

        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("az command failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn resolve_iteration_path_by_title_or_path(
        &self,
        milestone: &str,
    ) -> Result<Option<String>> {
        let json_str = self
            .run_az(&[
                "boards",
                "iteration",
                "project",
                "list",
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        let iterations = parse_iterations_json(&json_str)?;
        Ok(iterations
            .into_iter()
            .find(|iteration| {
                titles_equivalent(&iteration.title, milestone) || iteration.path == milestone
            })
            .map(|iteration| iteration.path))
    }
}

fn escape_wiql_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

struct AzOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[async_trait]
trait AzRunner: Send + Sync {
    async fn run(&self, args: &[&str]) -> Result<AzOutput>;
}

struct AzCliRunner;

#[async_trait]
impl AzRunner for AzCliRunner {
    async fn run(&self, args: &[&str]) -> Result<AzOutput> {
        let output = tokio::process::Command::new("az")
            .args(args)
            .output()
            .await?;

        Ok(AzOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// Parse Azure DevOps work items JSON into provider-agnostic Issue types.
pub fn parse_work_items_json(json_str: &str) -> Result<Vec<Issue>> {
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(json_str).context("Failed to parse Azure DevOps work items JSON")?;
    let mut issues = Vec::new();
    for v in raw {
        let number = v
            .get("id")
            .and_then(|n| n.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' field in work item JSON"))?;

        let fields = v.get("fields");

        let title = fields
            .and_then(|f| f.get("System.Title"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let body = fields
            .and_then(|f| f.get("System.Description"))
            .and_then(|b| b.as_str())
            .unwrap_or("")
            .to_string();

        let state_raw = fields
            .and_then(|f| f.get("System.State"))
            .and_then(|s| s.as_str())
            .unwrap_or("Active");

        let state = match state_raw {
            "Closed" | "Resolved" | "Done" | "Removed" => "closed",
            _ => "open",
        }
        .to_string();

        let tags_str = fields
            .and_then(|f| f.get("System.Tags"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let labels: Vec<String> = if tags_str.is_empty() {
            Vec::new()
        } else {
            tags_str
                .split("; ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        let html_url = v
            .get("url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        issues.push(Issue {
            number,
            title,
            body,
            labels,
            state,
            html_url,
            milestone: None,
            assignees: vec![],
        });
    }
    Ok(issues)
}

#[async_trait]
impl RepoProvider for AzDevOpsClient {
    async fn list_issues(&self, labels: &[&str]) -> Result<Vec<Issue>> {
        let mut wiql = String::from(
            "SELECT [System.Id] FROM WorkItems WHERE [System.State] <> 'Closed' \
             AND [System.State] <> 'Removed'",
        );
        for label in labels {
            wiql.push_str(&format!(" AND [System.Tags] CONTAINS '{}'", label));
        }

        let json_str = self
            .run_az(&[
                "boards",
                "query",
                "--wiql",
                &wiql,
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;

        // The query returns IDs; fetch full details
        let ids: Vec<serde_json::Value> = serde_json::from_str(&json_str)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let id_list: Vec<String> = ids
            .iter()
            .filter_map(|v| v.get("id").and_then(|n| n.as_u64()))
            .map(|n| n.to_string())
            .collect();

        if id_list.is_empty() {
            return Ok(Vec::new());
        }

        let details_str = self
            .run_az(&[
                "boards",
                "work-item",
                "show",
                "--ids",
                &id_list.join(" "),
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        parse_work_items_json(&details_str)
    }

    async fn list_issues_by_milestone(&self, milestone: &str) -> Result<Vec<Issue>> {
        let iteration_path = self
            .resolve_iteration_path_by_title_or_path(milestone)
            .await?
            .unwrap_or_else(|| milestone.to_string());
        let escaped_iteration_path = escape_wiql_string_literal(&iteration_path);
        let wiql = format!(
            "SELECT [System.Id] FROM WorkItems WHERE [System.IterationPath] UNDER '{}' \
             AND [System.State] <> 'Closed' AND [System.State] <> 'Removed'",
            escaped_iteration_path
        );

        let json_str = self
            .run_az(&[
                "boards",
                "query",
                "--wiql",
                &wiql,
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;

        let ids: Vec<serde_json::Value> = serde_json::from_str(&json_str)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let id_list: Vec<String> = ids
            .iter()
            .filter_map(|v| v.get("id").and_then(|n| n.as_u64()))
            .map(|n| n.to_string())
            .collect();

        if id_list.is_empty() {
            return Ok(Vec::new());
        }

        let details_str = self
            .run_az(&[
                "boards",
                "work-item",
                "show",
                "--ids",
                &id_list.join(" "),
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        parse_work_items_json(&details_str)
    }

    async fn list_milestones(&self, state: &str) -> Result<Vec<Milestone>> {
        let today = today_utc();
        let json_str = self
            .run_az(&[
                "boards",
                "iteration",
                "project",
                "list",
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        let iterations = parse_iterations_json(&json_str)?;
        let filtered = filter_iterations_by_state(iterations, state, today)?;
        Ok(iterations_to_milestones(filtered, today))
    }

    async fn get_issue(&self, number: u64) -> Result<Issue> {
        let num_str = number.to_string();
        let json_str = self
            .run_az(&[
                "boards",
                "work-item",
                "show",
                "--id",
                &num_str,
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        let issues = parse_work_items_json(&format!("[{}]", json_str))?;
        issues
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Work item #{} not found", number))
    }

    async fn add_label(&self, issue_number: u64, label: &str) -> Result<()> {
        // Read current tags, append new one
        let issue = self.get_issue(issue_number).await?;
        let mut tags: Vec<String> = issue.labels;
        if !tags.iter().any(|t| t == label) {
            tags.push(label.to_string());
        }
        let tags_str = tags.join("; ");
        let num_str = issue_number.to_string();
        let field = format!("System.Tags={}", tags_str);
        self.run_az(&[
            "boards",
            "work-item",
            "update",
            "--id",
            &num_str,
            "--fields",
            &field,
            "--org",
            &self.organization,
        ])
        .await?;
        Ok(())
    }

    async fn remove_label(&self, issue_number: u64, label: &str) -> Result<()> {
        let issue = self.get_issue(issue_number).await?;
        let tags: Vec<String> = issue.labels.into_iter().filter(|t| t != label).collect();
        let tags_str = tags.join("; ");
        let num_str = issue_number.to_string();
        let field = format!("System.Tags={}", tags_str);
        self.run_az(&[
            "boards",
            "work-item",
            "update",
            "--id",
            &num_str,
            "--fields",
            &field,
            "--org",
            &self.organization,
        ])
        .await?;
        Ok(())
    }

    async fn create_pr(
        &self,
        _issue_number: u64,
        title: &str,
        body: &str,
        head_branch: &str,
        base_branch: &str,
    ) -> Result<u64> {
        let json_str = self
            .run_az(&[
                "repos",
                "pr",
                "create",
                "--title",
                title,
                "--description",
                body,
                "--source-branch",
                head_branch,
                "--target-branch",
                base_branch,
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;

        let v: serde_json::Value = serde_json::from_str(&json_str)?;
        Ok(v.get("pullRequestId").and_then(|n| n.as_u64()).unwrap_or(0))
    }

    async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>> {
        let json_str = self
            .run_az(&[
                "repos",
                "pr",
                "list",
                "--source-branch",
                head_branch,
                "--status",
                "active",
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        let prs: Vec<serde_json::Value> = serde_json::from_str(&json_str)?;
        Ok(prs
            .iter()
            .filter_map(|v| v.get("pullRequestId").and_then(|n| n.as_u64()))
            .collect())
    }

    async fn create_milestone(&self, title: &str, description: &str) -> Result<CreateOutcome> {
        let normalized = validate_title(title, "milestone title")?;
        validate_body(description, "milestone description")?;
        let today = today_utc();
        let existing_json = self
            .run_az(&[
                "boards",
                "iteration",
                "project",
                "list",
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        let existing = parse_iterations_json(&existing_json)?;

        if let Some(iteration) = existing
            .iter()
            .find(|iteration| titles_equivalent(&iteration.title, &normalized))
        {
            return Ok(CreateOutcome::Existed {
                number: iteration.number,
                state: iteration_state(iteration, today),
            });
        }

        let created_json = self
            .run_az(&[
                "boards",
                "iteration",
                "project",
                "create",
                "--name",
                &normalized,
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        let created = parse_iteration_json(&created_json)?;

        self.run_az(&[
            "boards",
            "iteration",
            "project",
            "update",
            "--path",
            &created.path,
            "--description",
            description,
            "--org",
            &self.organization,
            "--project",
            &self.project,
            "-o",
            "json",
        ])
        .await?;

        Ok(CreateOutcome::Created(created.number))
    }

    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<CreateOutcome> {
        let normalized_title = validate_title(title, "issue title")?;
        let iteration_path = if let Some(milestone_number) = milestone {
            let json_str = self
                .run_az(&[
                    "boards",
                    "iteration",
                    "project",
                    "list",
                    "--org",
                    &self.organization,
                    "--project",
                    &self.project,
                    "-o",
                    "json",
                ])
                .await?;
            let iterations = parse_iterations_json(&json_str)?;
            Some(
                iteration_path_for_milestone_number(&iterations, milestone_number).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "Azure DevOps iteration for milestone id {milestone_number} not found"
                        )
                    },
                )?,
            )
        } else {
            None
        };
        let args = build_create_work_item_args(
            &self.organization,
            &self.project,
            &normalized_title,
            body,
            labels,
            iteration_path.as_deref(),
        );
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let json_str = self.run_az(&arg_refs).await?;
        let id = parse_created_work_item_id(&json_str)?;
        Ok(CreateOutcome::Created(id))
    }

    async fn list_open_prs(&self) -> Result<Vec<PullRequest>> {
        anyhow::bail!("list_open_prs is not supported for Azure DevOps")
    }

    async fn get_pr(&self, _number: u64) -> Result<PullRequest> {
        anyhow::bail!("get_pr is not supported for Azure DevOps")
    }

    async fn submit_pr_review(
        &self,
        _pr_number: u64,
        _event: ReviewEvent,
        _body: &str,
    ) -> Result<()> {
        anyhow::bail!("submit_pr_review is not supported for Azure DevOps")
    }

    async fn list_labels(&self) -> Result<Vec<String>> {
        anyhow::bail!("list_labels is not supported for Azure DevOps")
    }

    async fn create_label(&self, _name: &str, _color: &str) -> Result<()> {
        anyhow::bail!("create_label is not supported for Azure DevOps")
    }

    async fn patch_milestone_description(
        &self,
        _milestone_number: u64,
        _description: &str,
    ) -> Result<()> {
        anyhow::bail!("patch_milestone_description is not supported for Azure DevOps")
    }

    async fn ci_status_for_branch(&self, _branch: &str) -> Result<CiStatus> {
        anyhow::bail!("not yet implemented — tracked in v0.23.0 #B5")
    }

    async fn ci_status_for_pr(&self, _pr_number: u64) -> Result<CiStatus> {
        anyhow::bail!("not yet implemented — tracked in v0.23.0 #B5")
    }

    async fn ci_check_runs_for_pr(&self, _pr_number: u64) -> Result<Vec<CheckRun>> {
        anyhow::bail!("not yet implemented — tracked in v0.23.0 #B5")
    }

    async fn ci_logs_for_check(&self, _check_id: &str) -> Result<String> {
        anyhow::bail!("not yet implemented — tracked in v0.23.0 #B5")
    }

    async fn merge_pr(&self, _pr_number: u64, _method: MergeMethod) -> Result<()> {
        anyhow::bail!("not yet implemented — tracked in v0.23.0 #B5")
    }
}
