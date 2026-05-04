use crate::provider::github::client::{CreateOutcome, RepoProvider};
use crate::provider::types::{
    CheckRun, CiStatus, Issue, MergeMethod, Milestone, PullRequest, ReviewEvent,
};
use crate::util::{titles_equivalent, validate_body, validate_title};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::io::Write;
use std::sync::Arc;

mod ci;
mod issues;
mod iterations;
mod merge;
mod pr;

use ci::{aggregate_pipeline_runs, parse_check_runs_json, parse_pipeline_runs_json};
use issues::{build_create_work_item_args, parse_created_work_item_id};
use iterations::{
    filter_iterations_by_state, iteration_path_for_milestone_number, iteration_state,
    iterations_to_milestones, parse_iteration_json, parse_iterations_json, today_utc,
};
use merge::merge_method_args;
use pr::{parse_pr_json, parse_prs_json};

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

    async fn run_az_output(&self, args: &[&str]) -> Result<AzOutput> {
        let output =
            self.runner.run(args).await.context(
                "Failed to run `az` CLI. Is it installed? Run `az login` to authenticate.",
            )?;
        Ok(output)
    }

    async fn run_az(&self, args: &[&str]) -> Result<String> {
        let output = self.run_az_output(args).await?;

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

/// Parse Azure DevOps tag dictionary JSON into provider-agnostic label names.
pub fn parse_tags_json(json_str: &str) -> Result<Vec<String>> {
    if json_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    let raw: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse Azure DevOps tags JSON")?;
    let tags = match &raw {
        serde_json::Value::Array(items) => items.as_slice(),
        serde_json::Value::Object(map) => map
            .get("value")
            .and_then(|value| value.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        _ => &[],
    };

    Ok(tags
        .iter()
        .filter_map(|tag| match tag {
            serde_json::Value::String(name) => Some(name.as_str()),
            serde_json::Value::Object(map) => map.get("name").and_then(|name| name.as_str()),
            _ => None,
        })
        .map(str::to_string)
        .collect())
}

fn azure_tags_route_parameter(project: &str) -> String {
    format!("project={project}")
}

fn duplicate_tag_error(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("tag already exists")
}

fn duplicate_create_error(stderr: &str) -> bool {
    let normalized = stderr.to_ascii_lowercase();
    normalized.contains("already exists") || normalized.contains("duplicate")
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

        let create_args = [
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
        ];
        let output = self.run_az_output(&create_args).await?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if duplicate_create_error(&stderr) {
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
                anyhow::bail!(
                    "Azure DevOps iteration '{}' already exists but was not found by title lookup",
                    normalized
                );
            }
            anyhow::bail!("az command failed: {}", stderr.trim());
        }
        let created_json = String::from_utf8_lossy(&output.stdout).to_string();
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
        validate_body(body, "issue body")?;

        let existing_issues = self.list_issues(&[]).await?;
        if let Some(issue) = existing_issues
            .iter()
            .find(|issue| titles_equivalent(&issue.title, &normalized_title))
        {
            return Ok(CreateOutcome::Existed {
                number: issue.number,
                state: issue.state.clone(),
            });
        }

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
        let output = self.run_az_output(&arg_refs).await?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if duplicate_create_error(&stderr) {
                let existing_issues = self.list_issues(&[]).await?;
                if let Some(issue) = existing_issues
                    .iter()
                    .find(|issue| titles_equivalent(&issue.title, &normalized_title))
                {
                    return Ok(CreateOutcome::Existed {
                        number: issue.number,
                        state: issue.state.clone(),
                    });
                }
                anyhow::bail!(
                    "Azure DevOps work item '{}' already exists but was not found by title lookup",
                    normalized_title
                );
            }
            anyhow::bail!("az command failed: {}", stderr.trim());
        }
        let json_str = String::from_utf8_lossy(&output.stdout).to_string();
        let id = parse_created_work_item_id(&json_str)?;
        Ok(CreateOutcome::Created(id))
    }

    async fn list_open_prs(&self) -> Result<Vec<PullRequest>> {
        let json_str = self
            .run_az(&[
                "repos",
                "pr",
                "list",
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

        parse_prs_json(&json_str)
    }

    async fn get_pr(&self, number: u64) -> Result<PullRequest> {
        let number_str = number.to_string();
        let json_str = self
            .run_az(&[
                "repos",
                "pr",
                "show",
                "--id",
                &number_str,
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        parse_pr_json(&json_str)
    }

    async fn submit_pr_review(&self, pr_number: u64, event: ReviewEvent, body: &str) -> Result<()> {
        validate_body(body, "PR review body")?;
        let pr_number_str = pr_number.to_string();

        if let Some(vote) = pr::review_event_vote(event) {
            self.run_az(&[
                "repos",
                "pr",
                "set-vote",
                "--id",
                &pr_number_str,
                "--vote",
                vote,
                "--org",
                &self.organization,
                "--project",
                &self.project,
            ])
            .await?;
        }

        self.run_az(&[
            "repos",
            "pr",
            "comment",
            "add",
            "--id",
            &pr_number_str,
            "--content",
            body,
            "--org",
            &self.organization,
            "--project",
            &self.project,
        ])
        .await?;

        Ok(())
    }

    async fn list_labels(&self) -> Result<Vec<String>> {
        let route_project = azure_tags_route_parameter(&self.project);
        let json_str = self
            .run_az(&[
                "devops",
                "invoke",
                "--area",
                "wit",
                "--resource",
                "tags",
                "--route-parameters",
                &route_project,
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        parse_tags_json(&json_str)
    }

    async fn create_label(&self, name: &str, color: &str) -> Result<()> {
        tracing::debug!(
            label = name,
            color,
            "Azure DevOps work-item tags do not support label colors; ignoring color"
        );

        let mut body_file =
            tempfile::NamedTempFile::new().context("Failed to create Azure DevOps tag payload")?;
        let body = serde_json::to_string(&serde_json::json!({ "name": name }))?;
        body_file
            .write_all(body.as_bytes())
            .context("Failed to write Azure DevOps tag payload")?;
        body_file
            .flush()
            .context("Failed to flush Azure DevOps tag payload")?;

        let route_project = azure_tags_route_parameter(&self.project);
        let body_path = body_file
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Azure DevOps tag payload path is not UTF-8"))?;
        let output = self
            .run_az_output(&[
                "devops",
                "invoke",
                "--area",
                "wit",
                "--resource",
                "tags",
                "--route-parameters",
                &route_project,
                "--http-method",
                "PATCH",
                "--in-file",
                body_path,
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if duplicate_tag_error(&stderr) {
                return Ok(());
            }
            anyhow::bail!("az command failed: {}", stderr.trim());
        }

        Ok(())
    }

    async fn patch_milestone_description(
        &self,
        _milestone_number: u64,
        _description: &str,
    ) -> Result<()> {
        anyhow::bail!("patch_milestone_description is not supported for Azure DevOps")
    }

    async fn ci_status_for_branch(&self, branch: &str) -> Result<CiStatus> {
        crate::util::validate_gh_arg(branch, "branch")?;
        let branch_ref = ci::refs_heads_branch(branch);
        let json_str = self
            .run_az(&[
                "pipelines",
                "runs",
                "list",
                "--branch",
                &branch_ref,
                "--top",
                "20",
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        Ok(aggregate_pipeline_runs(parse_pipeline_runs_json(
            &json_str,
        )?))
    }

    async fn ci_status_for_pr(&self, pr_number: u64) -> Result<CiStatus> {
        let pr = self.get_pr(pr_number).await?;
        self.ci_status_for_branch(&pr.head_branch).await
    }

    async fn ci_check_runs_for_pr(&self, pr_number: u64) -> Result<Vec<CheckRun>> {
        let pr = self.get_pr(pr_number).await?;
        let branch_ref = ci::refs_heads_branch(&pr.head_branch);
        let json_str = self
            .run_az(&[
                "pipelines",
                "runs",
                "list",
                "--branch",
                &branch_ref,
                "--top",
                "20",
                "--org",
                &self.organization,
                "--project",
                &self.project,
                "-o",
                "json",
            ])
            .await?;
        parse_check_runs_json(&json_str)
    }

    async fn ci_logs_for_check(&self, check_id: &str) -> Result<String> {
        crate::util::validate_gh_arg(check_id, "check_id")?;
        self.run_az(&[
            "pipelines",
            "runs",
            "show",
            "--id",
            check_id,
            "--org",
            &self.organization,
            "-o",
            "json",
        ])
        .await?;

        let logs_output = self
            .run_az_output(&[
                "pipelines",
                "runs",
                "show",
                "--id",
                check_id,
                "--query",
                "logs",
                "--org",
                &self.organization,
                "-o",
                "json",
            ])
            .await?;

        if logs_output.success {
            return Ok(String::from_utf8_lossy(&logs_output.stdout).to_string());
        }

        let route_project = format!("project={}", self.project);
        let route_build = format!("buildId={check_id}");
        self.run_az(&[
            "devops",
            "invoke",
            "--area",
            "build",
            "--resource",
            "logs",
            "--route-parameters",
            &route_project,
            &route_build,
            "--org",
            &self.organization,
            "-o",
            "json",
        ])
        .await
    }

    async fn merge_pr(&self, pr_number: u64, method: MergeMethod) -> Result<()> {
        let pr = self.get_pr(pr_number).await?;
        if pr.draft {
            anyhow::bail!(
                "Azure DevOps PR #{} is a draft and cannot be completed",
                pr_number
            );
        }
        if pr.state != "active" {
            anyhow::bail!(
                "Azure DevOps PR #{} is '{}' and cannot be completed",
                pr_number,
                pr.state
            );
        }

        let pr_number_str = pr_number.to_string();
        let mut args = vec![
            "repos",
            "pr",
            "update",
            "--id",
            &pr_number_str,
            "--status",
            "completed",
            "--merge-commit-message-mode",
            "default",
        ];
        args.extend(merge_method_args(method));
        args.extend(["--org", &self.organization, "--project", &self.project]);
        self.run_az(&args).await?;
        Ok(())
    }
}
