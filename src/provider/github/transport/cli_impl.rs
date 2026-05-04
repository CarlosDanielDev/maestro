use super::{
    CreateOutcome, GhCliClient, GitHubClient, PR_JSON_FIELDS, argv_refs, is_label_not_found_error,
    normalize_paginated_json_arrays, parse_issues_json, parse_milestones_json,
    parse_pr_number_from_create_output, parse_prs_json,
};
use crate::provider::github::gh_argv;
use crate::provider::github::types::{GhIssue, GhMilestone, GhPullRequest, PrReviewEvent};
use crate::util::{titles_equivalent, validate_body, validate_gh_arg, validate_title};
use anyhow::{Context, Result};
use async_trait::async_trait;

#[async_trait]
impl GitHubClient for GhCliClient {
    async fn list_issues(&self, labels: &[&str]) -> Result<Vec<GhIssue>> {
        for label in labels {
            validate_gh_arg(label, "label")?;
        }
        let label_arg = labels.join(",");
        let labels_csv = if label_arg.is_empty() {
            None
        } else {
            Some(label_arg.as_str())
        };
        let argv = gh_argv::build_list_issues_argv(labels_csv, self.repo_arg());
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        parse_issues_json(&json_str)
    }

    async fn list_issues_by_milestone(&self, milestone: &str) -> Result<Vec<GhIssue>> {
        validate_gh_arg(milestone, "milestone")?;
        let argv = gh_argv::build_list_issues_by_milestone_argv(milestone, self.repo_arg());
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        parse_issues_json(&json_str)
    }

    async fn list_milestones(&self, state: &str) -> Result<Vec<GhMilestone>> {
        match state {
            "open" | "closed" | "all" => {}
            _ => anyhow::bail!(
                "Invalid milestone state: {:?}. Must be open, closed, or all",
                state
            ),
        }
        let argv = gh_argv::build_list_milestones_argv(state);
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        let normalized = normalize_paginated_json_arrays(&json_str)?;
        parse_milestones_json(&normalized)
    }

    async fn get_issue(&self, number: u64) -> Result<GhIssue> {
        let argv = gh_argv::build_get_issue_argv(number, self.repo_arg());
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        // gh issue view returns a single object, wrap it in array for parsing
        let issues = parse_issues_json(&format!("[{}]", json_str))?;
        issues
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Issue #{} not found", number))
    }

    async fn add_label(&self, issue_number: u64, label: &str) -> Result<()> {
        validate_gh_arg(label, "label")?;
        let argv = gh_argv::build_add_label_argv(issue_number, label, self.repo_arg());
        let result = self.run_gh(&argv_refs(&argv)).await;

        if let Err(ref e) = result {
            let err_msg = e.to_string();
            // If the label doesn't exist, create it and retry
            if err_msg.contains("not found") || err_msg.contains("label") {
                let color = match label {
                    "maestro:ready" => "0E8A16",
                    "maestro:in-progress" => "F9D0C4",
                    "maestro:done" => "0E8A16",
                    "maestro:failed" => "D93F0B",
                    _ => "EDEDED",
                };
                let _ = self.create_label(label, color).await;
                // Retry adding the label
                self.run_gh(&argv_refs(&argv)).await?;
                return Ok(());
            }
        }
        result.map(|_| ())
    }

    async fn remove_label(&self, issue_number: u64, label: &str) -> Result<()> {
        validate_gh_arg(label, "label")?;
        let argv = gh_argv::build_remove_label_argv(issue_number, label, self.repo_arg());
        match self.run_gh(&argv_refs(&argv)).await {
            Ok(_) => Ok(()),
            Err(e) if is_label_not_found_error(&e.to_string(), label) => {
                tracing::debug!(
                    issue = issue_number,
                    label = label,
                    "remove_label: label not found on repo or issue — treating as no-op"
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn create_pr(
        &self,
        _issue_number: u64,
        title: &str,
        body: &str,
        head_branch: &str,
        base_branch: &str,
    ) -> Result<u64> {
        let normalized_title = validate_title(title, "PR title")?;
        validate_body(body, "PR body")?;
        validate_gh_arg(head_branch, "head_branch")?;
        validate_gh_arg(base_branch, "base_branch")?;
        let argv = gh_argv::build_create_pr_argv(head_branch, base_branch, &normalized_title, body);
        let stdout = self.run_gh(&argv_refs(&argv)).await?;
        parse_pr_number_from_create_output(&stdout)
    }

    async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>> {
        validate_gh_arg(head_branch, "head_branch")?;
        let argv = gh_argv::build_list_prs_for_branch_argv(head_branch, self.repo_arg());
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        let prs: Vec<serde_json::Value> = serde_json::from_str(&json_str)?;
        Ok(prs
            .iter()
            .filter_map(|v| v.get("number").and_then(|n| n.as_u64()))
            .collect())
    }

    async fn create_milestone(&self, title: &str, description: &str) -> Result<CreateOutcome> {
        let normalized = validate_title(title, "milestone title")?;
        validate_body(description, "milestone description")?;

        let mut open = self.list_milestones("open").await?;
        let closed = self.list_milestones("closed").await?;
        open.extend(closed);
        if let Some(existing) = open
            .iter()
            .find(|m| titles_equivalent(&m.title, &normalized))
        {
            tracing::info!(
                milestone = %normalized,
                number = existing.number,
                state = %existing.state,
                "create_milestone proactive hit — reusing existing milestone"
            );
            return Ok(CreateOutcome::Existed {
                number: existing.number,
                state: existing.state.clone(),
            });
        }

        let argv = gh_argv::build_create_milestone_argv(&normalized, description);
        let result = self.run_gh(&argv_refs(&argv)).await;

        match result {
            Ok(json_str) => {
                let v: serde_json::Value = serde_json::from_str(&json_str)
                    .context("Failed to parse milestone response")?;
                let number = v
                    .get("number")
                    .and_then(|n| n.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'number' in milestone response"))?;
                Ok(CreateOutcome::Created(number))
            }
            Err(e) => {
                let msg = e.to_string();
                // 422 = duplicate milestone race — re-fetch open + closed
                // and resolve via titles_equivalent.
                if msg.contains("422") || msg.contains("Validation Failed") {
                    let mut all = self.list_milestones("open").await?;
                    let closed = self.list_milestones("closed").await?;
                    all.extend(closed);
                    if let Some(existing) = all
                        .iter()
                        .find(|m| titles_equivalent(&m.title, &normalized))
                    {
                        tracing::info!(
                            milestone = %normalized,
                            number = existing.number,
                            state = %existing.state,
                            "create_milestone 422 recovery — matched existing milestone"
                        );
                        return Ok(CreateOutcome::Existed {
                            number: existing.number,
                            state: existing.state.clone(),
                        });
                    }
                    Err(anyhow::anyhow!(
                        "Milestone '{}' caused 422 but not found in open or closed milestones",
                        normalized
                    ))
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<CreateOutcome> {
        let normalized = validate_title(title, "issue title")?;
        validate_body(body, "issue body")?;

        // Proactive pre-check: scan open + closed issues for an equivalent title.
        let dupe_argv = gh_argv::build_create_issue_dupe_check_argv(self.repo_arg());
        let all_json = self.run_gh(&argv_refs(&dupe_argv)).await?;
        let all_issues: Vec<serde_json::Value> = serde_json::from_str(&all_json)
            .context("Failed to parse issue list JSON for dupe pre-check")?;
        for v in &all_issues {
            let existing_title = v.get("title").and_then(|t| t.as_str()).unwrap_or("");
            if titles_equivalent(existing_title, &normalized) {
                let number = v.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
                let state = v
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("open")
                    .to_lowercase();
                tracing::info!(
                    issue = %normalized,
                    number,
                    state = %state,
                    "create_issue proactive hit — reusing existing issue"
                );
                return Ok(CreateOutcome::Existed { number, state });
            }
        }

        // Use REST API via stdin because `gh issue create --milestone`
        // expects a title string, but we only have the milestone number.
        let mut payload = serde_json::json!({
            "title": normalized,
            "body": body,
        });
        if !labels.is_empty() {
            payload["labels"] = serde_json::json!(labels);
        }
        if let Some(ms) = milestone {
            payload["milestone"] = serde_json::json!(ms);
        }

        let json_body = serde_json::to_string(&payload)?;
        let create_argv = gh_argv::build_create_issue_argv();
        let json_str = self
            .run_gh_with_stdin(&argv_refs(&create_argv), Some(json_body.as_bytes()))
            .await?;

        let v: serde_json::Value =
            serde_json::from_str(&json_str).context("Failed to parse issue creation response")?;
        let number = v
            .get("number")
            .and_then(|n| n.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'number' in issue creation response"))?;
        Ok(CreateOutcome::Created(number))
    }

    async fn list_open_prs(&self) -> Result<Vec<GhPullRequest>> {
        let argv = gh_argv::build_list_open_prs_argv(PR_JSON_FIELDS, self.repo_arg());
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        parse_prs_json(&json_str)
    }

    async fn get_pr(&self, number: u64) -> Result<GhPullRequest> {
        let argv = gh_argv::build_get_pr_argv(number, PR_JSON_FIELDS, self.repo_arg());
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        let prs = parse_prs_json(&format!("[{}]", json_str))?;
        prs.into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("PR #{} not found", number))
    }

    async fn submit_pr_review(
        &self,
        pr_number: u64,
        event: PrReviewEvent,
        body: &str,
    ) -> Result<()> {
        // Parity with create_pr / create_issue / create_milestone — every
        // user-facing body that ships to GitHub is bounded at the same cap.
        validate_body(body, "PR review body")?;
        let argv = gh_argv::build_submit_pr_review_argv(pr_number, event, body);
        self.run_gh(&argv_refs(&argv)).await?;
        Ok(())
    }

    async fn list_labels(&self) -> Result<Vec<String>> {
        let argv = gh_argv::build_list_labels_argv();
        let json_str = self.run_gh(&argv_refs(&argv)).await?;
        let labels: Vec<serde_json::Value> =
            serde_json::from_str(&json_str).context("Failed to parse label list JSON")?;
        Ok(labels
            .iter()
            .filter_map(|v| {
                v.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect())
    }

    async fn create_label(&self, name: &str, color: &str) -> Result<()> {
        validate_gh_arg(name, "label name")?;
        validate_gh_arg(color, "label color")?;
        let argv = gh_argv::build_create_label_argv(name, color);
        self.run_gh(&argv_refs(&argv)).await?;
        Ok(())
    }

    async fn patch_milestone_description(
        &self,
        milestone_number: u64,
        description: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({ "description": description });
        let json_body = serde_json::to_string(&payload)?;
        let argv = gh_argv::build_patch_milestone_description_argv(milestone_number);
        self.run_gh_with_stdin(&argv_refs(&argv), Some(json_body.as_bytes()))
            .await
            .with_context(|| format!("patching milestone #{} description", milestone_number))?;
        Ok(())
    }
}
