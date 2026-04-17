use super::types::Issue;
use crate::provider::github::client::GitHubClient;
use crate::provider::github::types::GhMilestone;
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Azure DevOps client using `az` CLI.
pub struct AzDevOpsClient {
    organization: String,
    project: String,
}

impl AzDevOpsClient {
    pub fn new(organization: String, project: String) -> Self {
        Self {
            organization,
            project,
        }
    }

    async fn run_az(&self, args: &[&str]) -> Result<String> {
        let output = tokio::process::Command::new("az")
            .args(args)
            .output()
            .await
            .context("Failed to run `az` CLI. Is it installed? Run `az login` to authenticate.")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("az command failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
impl GitHubClient for AzDevOpsClient {
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
        let wiql = format!(
            "SELECT [System.Id] FROM WorkItems WHERE [System.IterationPath] UNDER '{}' \
             AND [System.State] <> 'Closed' AND [System.State] <> 'Removed'",
            milestone
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

    async fn list_milestones(&self, _state: &str) -> Result<Vec<GhMilestone>> {
        // Azure DevOps uses iterations rather than milestones; not yet implemented.
        Ok(vec![])
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

    async fn create_milestone(&self, _title: &str, _description: &str) -> Result<u64> {
        anyhow::bail!("create_milestone is not supported for Azure DevOps")
    }

    async fn create_issue(
        &self,
        _title: &str,
        _body: &str,
        _labels: &[String],
        _milestone: Option<u64>,
    ) -> Result<u64> {
        anyhow::bail!("create_issue is not supported for Azure DevOps")
    }

    async fn list_open_prs(&self) -> Result<Vec<crate::provider::github::types::GhPullRequest>> {
        anyhow::bail!("list_open_prs is not supported for Azure DevOps")
    }

    async fn get_pr(&self, _number: u64) -> Result<crate::provider::github::types::GhPullRequest> {
        anyhow::bail!("get_pr is not supported for Azure DevOps")
    }

    async fn submit_pr_review(
        &self,
        _pr_number: u64,
        _event: crate::provider::github::types::PrReviewEvent,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_work_items_json_valid_single_item() {
        let json = r#"[{
            "id": 101,
            "fields": {
                "System.Title": "Fix login bug",
                "System.Description": "Detailed description",
                "System.State": "Active",
                "System.Tags": "maestro:ready; priority:P1"
            },
            "url": "https://dev.azure.com/MyOrg/MyProject/_apis/wit/workItems/101"
        }]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 101);
        assert_eq!(issues[0].title, "Fix login bug");
        assert_eq!(issues[0].state, "open");
    }

    #[test]
    fn parse_work_items_json_maps_labels_from_tags() {
        let json = r#"[{
            "id": 1,
            "fields": {
                "System.Title": "T",
                "System.State": "Active",
                "System.Tags": "maestro:ready; priority:P1"
            },
            "url": ""
        }]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert_eq!(issues[0].labels, vec!["maestro:ready", "priority:P1"]);
    }

    #[test]
    fn parse_work_items_json_empty_tags_produces_empty_labels() {
        let json = r#"[{
            "id": 1,
            "fields": {"System.Title": "T", "System.State": "Active", "System.Tags": ""},
            "url": ""
        }]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert!(issues[0].labels.is_empty());
    }

    #[test]
    fn parse_work_items_json_active_state_maps_to_open() {
        let json = r#"[{"id":1,"fields":{"System.Title":"T","System.State":"Active"},"url":""}]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert_eq!(issues[0].state, "open");
    }

    #[test]
    fn parse_work_items_json_closed_state_maps_to_closed() {
        let json = r#"[{"id":1,"fields":{"System.Title":"T","System.State":"Closed"},"url":""}]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert_eq!(issues[0].state, "closed");
    }

    #[test]
    fn parse_work_items_json_resolved_state_maps_to_closed() {
        let json = r#"[{"id":1,"fields":{"System.Title":"T","System.State":"Resolved"},"url":""}]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert_eq!(issues[0].state, "closed");
    }

    #[test]
    fn parse_work_items_json_empty_array() {
        let issues = parse_work_items_json("[]").unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn parse_work_items_json_invalid_json_returns_err() {
        assert!(parse_work_items_json("{not json}").is_err());
    }

    #[test]
    fn parse_work_items_json_missing_id_returns_err() {
        let json = r#"[{"fields":{"System.Title":"T","System.State":"Active"},"url":""}]"#;
        assert!(parse_work_items_json(json).is_err());
    }

    #[test]
    fn parse_work_items_json_captures_url() {
        let json = r#"[{
            "id": 42,
            "fields": {"System.Title": "T", "System.State": "Active"},
            "url": "https://dev.azure.com/MyOrg/MyProject/_apis/wit/workItems/42"
        }]"#;
        let issues = parse_work_items_json(json).unwrap();
        assert_eq!(
            issues[0].html_url,
            "https://dev.azure.com/MyOrg/MyProject/_apis/wit/workItems/42"
        );
    }
}
