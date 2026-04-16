use super::types::{GhIssue, GhMilestone};
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Trait for GitHub API operations. Mockable for testing.
#[async_trait]
pub trait GitHubClient: Send + Sync {
    async fn list_issues(&self, labels: &[&str]) -> Result<Vec<GhIssue>>;
    async fn list_issues_by_milestone(&self, milestone: &str) -> Result<Vec<GhIssue>>;
    async fn list_milestones(&self, state: &str) -> Result<Vec<GhMilestone>>;
    async fn get_issue(&self, number: u64) -> Result<GhIssue>;
    async fn add_label(&self, issue_number: u64, label: &str) -> Result<()>;
    async fn remove_label(&self, issue_number: u64, label: &str) -> Result<()>;
    async fn create_pr(
        &self,
        issue_number: u64,
        title: &str,
        body: &str,
        head_branch: &str,
        base_branch: &str,
    ) -> Result<u64>;
    /// List open PR numbers for a given head branch.
    #[allow(dead_code)] // Reason: orphan branch detection feature
    async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>>;

    /// Create a GitHub milestone and return its number.
    async fn create_milestone(&self, title: &str, description: &str) -> Result<u64>;

    /// Create a GitHub issue and return its number.
    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<u64>;

    /// List open pull requests for the current repository.
    async fn list_open_prs(&self) -> Result<Vec<crate::github::types::GhPullRequest>>;

    /// Get a single pull request by number.
    #[allow(dead_code)] // Reason: PR detail view — currently PR data comes from list
    async fn get_pr(&self, number: u64) -> Result<crate::github::types::GhPullRequest>;

    /// Submit a review on a pull request.
    async fn submit_pr_review(
        &self,
        pr_number: u64,
        event: crate::github::types::PrReviewEvent,
        body: &str,
    ) -> Result<()>;

    /// List all label names on the current repository.
    async fn list_labels(&self) -> Result<Vec<String>>;

    /// Create a label on the current repository. Uses --force to be idempotent.
    async fn create_label(&self, name: &str, color: &str) -> Result<()>;
}

/// Extract label names from a JSON value containing `{"labels": [{"name": "..."}, ...]}`.
fn extract_label_names(v: &serde_json::Value) -> Vec<String> {
    v.get("labels")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|lv| {
                    lv.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse JSON output from `gh issue list --json ...`.
pub fn parse_issues_json(json_str: &str) -> Result<Vec<GhIssue>> {
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(json_str).context("Failed to parse GitHub issues JSON")?;
    let mut issues = Vec::new();
    for v in raw {
        let labels = extract_label_names(&v);

        let milestone = v.get("milestone").and_then(|m| {
            if m.is_null() {
                None
            } else if let Some(n) = m.as_u64() {
                Some(n)
            } else {
                m.get("number").and_then(|n| n.as_u64())
            }
        });

        let assignees: Vec<String> = v
            .get("assignees")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|av| {
                        av.get("login")
                            .and_then(|l| l.as_str())
                            .or_else(|| av.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        issues.push(GhIssue {
            number: v
                .get("number")
                .and_then(|n| n.as_u64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'number' field in issue JSON"))?,
            title: v
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            body: v
                .get("body")
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string(),
            labels,
            state: v
                .get("state")
                .and_then(|s| s.as_str())
                .unwrap_or("open")
                .to_lowercase(),
            html_url: v
                .get("html_url")
                .or_else(|| v.get("url"))
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string(),
            milestone,
            assignees,
        });
    }
    Ok(issues)
}

/// Parse JSON output from `gh api repos/{owner}/{repo}/milestones`.
pub fn parse_milestones_json(json_str: &str) -> Result<Vec<GhMilestone>> {
    serde_json::from_str(json_str).context("Failed to parse milestones JSON")
}

/// Parse JSON output from `gh pr list --json ...`.
pub fn parse_prs_json(json_str: &str) -> Result<Vec<crate::github::types::GhPullRequest>> {
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(json_str).context("Failed to parse GitHub PRs JSON")?;
    let mut prs = Vec::new();
    for v in raw {
        let labels = extract_label_names(&v);

        let author = v
            .get("author")
            .and_then(|a| {
                if a.is_string() {
                    a.as_str().map(|s| s.to_string())
                } else {
                    a.get("login")
                        .and_then(|l| l.as_str())
                        .map(|s| s.to_string())
                }
            })
            .unwrap_or_default();

        prs.push(crate::github::types::GhPullRequest {
            number: v
                .get("number")
                .and_then(|n| n.as_u64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'number' field in PR JSON"))?,
            title: v
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            body: v
                .get("body")
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string(),
            state: v
                .get("state")
                .and_then(|s| s.as_str())
                .unwrap_or("open")
                .to_lowercase(),
            html_url: v
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string(),
            head_branch: v
                .get("headRefName")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_string(),
            base_branch: v
                .get("baseRefName")
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string(),
            author,
            labels,
            draft: v.get("isDraft").and_then(|d| d.as_bool()).unwrap_or(false),
            mergeable: v
                .get("mergeable")
                .and_then(|m| {
                    if m.is_boolean() {
                        m.as_bool()
                    } else {
                        m.as_str().map(|s| s == "MERGEABLE")
                    }
                })
                .unwrap_or(false),
            additions: v.get("additions").and_then(|a| a.as_u64()).unwrap_or(0),
            deletions: v.get("deletions").and_then(|d| d.as_u64()).unwrap_or(0),
            changed_files: v.get("changedFiles").and_then(|c| c.as_u64()).unwrap_or(0),
        });
    }
    Ok(prs)
}

/// Blanket impl: if T: GitHubClient, then &T is also a GitHubClient.
#[async_trait]
impl<T: GitHubClient + ?Sized> GitHubClient for &T {
    async fn list_issues(&self, labels: &[&str]) -> Result<Vec<GhIssue>> {
        (**self).list_issues(labels).await
    }
    async fn list_issues_by_milestone(&self, milestone: &str) -> Result<Vec<GhIssue>> {
        (**self).list_issues_by_milestone(milestone).await
    }
    async fn list_milestones(&self, state: &str) -> Result<Vec<GhMilestone>> {
        (**self).list_milestones(state).await
    }
    async fn get_issue(&self, number: u64) -> Result<GhIssue> {
        (**self).get_issue(number).await
    }
    async fn add_label(&self, issue_number: u64, label: &str) -> Result<()> {
        (**self).add_label(issue_number, label).await
    }
    async fn remove_label(&self, issue_number: u64, label: &str) -> Result<()> {
        (**self).remove_label(issue_number, label).await
    }
    async fn create_pr(
        &self,
        issue_number: u64,
        title: &str,
        body: &str,
        head_branch: &str,
        base_branch: &str,
    ) -> Result<u64> {
        (**self)
            .create_pr(issue_number, title, body, head_branch, base_branch)
            .await
    }
    async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>> {
        (**self).list_prs_for_branch(head_branch).await
    }
    async fn create_milestone(&self, title: &str, description: &str) -> Result<u64> {
        (**self).create_milestone(title, description).await
    }
    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<u64> {
        (**self).create_issue(title, body, labels, milestone).await
    }
    async fn list_open_prs(&self) -> Result<Vec<crate::github::types::GhPullRequest>> {
        (**self).list_open_prs().await
    }
    async fn get_pr(&self, number: u64) -> Result<crate::github::types::GhPullRequest> {
        (**self).get_pr(number).await
    }
    async fn submit_pr_review(
        &self,
        pr_number: u64,
        event: crate::github::types::PrReviewEvent,
        body: &str,
    ) -> Result<()> {
        (**self).submit_pr_review(pr_number, event, body).await
    }
    async fn list_labels(&self) -> Result<Vec<String>> {
        (**self).list_labels().await
    }
    async fn create_label(&self, name: &str, color: &str) -> Result<()> {
        (**self).create_label(name, color).await
    }
}

/// Check if a stderr string indicates a GitHub CLI authentication failure.
pub fn is_auth_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("not logged in")
        || lower.contains("authentication required")
        || lower.contains("http 401")
        || lower.contains("auth login")
        || lower.contains("try authenticating")
        || lower.contains("authentication token")
        || lower.contains("could not authenticate")
}

/// Sentinel prefix used to tag gh auth errors in anyhow messages.
const GH_AUTH_ERROR_SENTINEL: &str = "[gh-auth-error]";

/// JSON fields requested from `gh pr list/view`.
const PR_JSON_FIELDS: &str = "number,title,body,state,url,headRefName,baseRefName,author,labels,isDraft,mergeable,additions,deletions,changedFiles";

/// Check if an anyhow error is a gh CLI auth error (by sentinel prefix).
pub fn is_gh_auth_error(err: &anyhow::Error) -> bool {
    err.to_string().contains(GH_AUTH_ERROR_SENTINEL)
}

use crate::util::validate_gh_arg;

/// Implementation that shells out to `gh` CLI.
pub struct GhCliClient;

impl GhCliClient {
    pub fn new() -> Self {
        Self
    }

    async fn run_gh(&self, args: &[&str]) -> Result<String> {
        self.run_gh_with_stdin(args, None).await
    }

    async fn run_gh_with_stdin(&self, args: &[&str], stdin_data: Option<&[u8]>) -> Result<String> {
        let stdin_cfg = if stdin_data.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        };

        let mut child = tokio::process::Command::new("gh")
            .args(args)
            .stdin(stdin_cfg)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to run `gh` CLI. Is it installed?")?;

        if let Some(data) = stdin_data
            && let Some(mut stdin) = child.stdin.take()
        {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(data).await?;
        }

        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for `gh` CLI")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if is_auth_error(&stderr) {
                anyhow::bail!("{} {}", GH_AUTH_ERROR_SENTINEL, stderr.trim());
            }
            anyhow::bail!("gh command failed: {}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait]
impl GitHubClient for GhCliClient {
    async fn list_issues(&self, labels: &[&str]) -> Result<Vec<GhIssue>> {
        for label in labels {
            validate_gh_arg(label, "label")?;
        }
        let label_arg = labels.join(",");
        let mut args = vec![
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,body,labels,state,url,milestone",
        ];
        if !label_arg.is_empty() {
            args.push("--label");
            args.push(&label_arg);
        }
        let json_str = self.run_gh(&args).await?;
        parse_issues_json(&json_str)
    }

    async fn list_issues_by_milestone(&self, milestone: &str) -> Result<Vec<GhIssue>> {
        validate_gh_arg(milestone, "milestone")?;
        let json_str = self
            .run_gh(&[
                "issue",
                "list",
                "--milestone",
                milestone,
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,body,labels,state,url,milestone",
            ])
            .await?;
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
        let endpoint = format!("repos/{{owner}}/{{repo}}/milestones?state={}", state);
        let json_str = self.run_gh(&["api", &endpoint, "--paginate"]).await?;
        parse_milestones_json(&json_str)
    }

    async fn get_issue(&self, number: u64) -> Result<GhIssue> {
        let num_str = number.to_string();
        let json_str = self
            .run_gh(&[
                "issue",
                "view",
                &num_str,
                "--json",
                "number,title,body,labels,state,url",
            ])
            .await?;
        // gh issue view returns a single object, wrap it in array for parsing
        let issues = parse_issues_json(&format!("[{}]", json_str))?;
        issues
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Issue #{} not found", number))
    }

    async fn add_label(&self, issue_number: u64, label: &str) -> Result<()> {
        validate_gh_arg(label, "label")?;
        let num_str = issue_number.to_string();
        let result = self
            .run_gh(&["issue", "edit", &num_str, "--add-label", label])
            .await;

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
                self.run_gh(&["issue", "edit", &num_str, "--add-label", label])
                    .await?;
                return Ok(());
            }
        }
        result.map(|_| ())
    }

    async fn remove_label(&self, issue_number: u64, label: &str) -> Result<()> {
        validate_gh_arg(label, "label")?;
        let num_str = issue_number.to_string();
        self.run_gh(&["issue", "edit", &num_str, "--remove-label", label])
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
        validate_gh_arg(head_branch, "head_branch")?;
        validate_gh_arg(base_branch, "base_branch")?;
        let json_str = self
            .run_gh(&[
                "pr",
                "create",
                "--head",
                head_branch,
                "--base",
                base_branch,
                "--title",
                title,
                "--body",
                body,
                "--json",
                "number",
            ])
            .await?;
        let v: serde_json::Value = serde_json::from_str(&json_str)?;
        Ok(v.get("number").and_then(|n| n.as_u64()).unwrap_or(0))
    }

    async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>> {
        validate_gh_arg(head_branch, "head_branch")?;
        let json_str = self
            .run_gh(&[
                "pr",
                "list",
                "--head",
                head_branch,
                "--state",
                "open",
                "--json",
                "number",
            ])
            .await?;
        let prs: Vec<serde_json::Value> = serde_json::from_str(&json_str)?;
        Ok(prs
            .iter()
            .filter_map(|v| v.get("number").and_then(|n| n.as_u64()))
            .collect())
    }

    async fn create_milestone(&self, title: &str, description: &str) -> Result<u64> {
        validate_gh_arg(title, "milestone title")?;
        let result = self
            .run_gh(&[
                "api",
                "repos/{owner}/{repo}/milestones",
                "--method",
                "POST",
                "-f",
                &format!("title={}", title),
                "-f",
                &format!("description={}", description),
            ])
            .await;

        match result {
            Ok(json_str) => {
                let v: serde_json::Value = serde_json::from_str(&json_str)
                    .context("Failed to parse milestone response")?;
                v.get("number")
                    .and_then(|n| n.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'number' in milestone response"))
            }
            Err(e) => {
                let msg = e.to_string();
                // 422 = duplicate milestone — find-or-reuse the existing one
                if msg.contains("422") || msg.contains("Validation Failed") {
                    let milestones = self.list_milestones("open").await?;
                    milestones
                        .iter()
                        .find(|m| m.title == title)
                        .map(|m| m.number)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Milestone '{}' caused 422 but not found in open milestones",
                                title
                            )
                        })
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
    ) -> Result<u64> {
        validate_gh_arg(title, "issue title")?;
        // Use REST API via stdin because `gh issue create --milestone`
        // expects a title string, but we only have the milestone number.
        let mut payload = serde_json::json!({
            "title": title,
            "body": body,
        });
        if !labels.is_empty() {
            payload["labels"] = serde_json::json!(labels);
        }
        if let Some(ms) = milestone {
            payload["milestone"] = serde_json::json!(ms);
        }

        let json_body = serde_json::to_string(&payload)?;
        let json_str = self
            .run_gh_with_stdin(
                &[
                    "api",
                    "repos/{owner}/{repo}/issues",
                    "--method",
                    "POST",
                    "--input",
                    "-",
                ],
                Some(json_body.as_bytes()),
            )
            .await?;

        let v: serde_json::Value =
            serde_json::from_str(&json_str).context("Failed to parse issue creation response")?;
        v.get("number")
            .and_then(|n| n.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'number' in issue creation response"))
    }

    async fn list_open_prs(&self) -> Result<Vec<crate::github::types::GhPullRequest>> {
        let json_str = self
            .run_gh(&[
                "pr",
                "list",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                PR_JSON_FIELDS,
            ])
            .await?;
        parse_prs_json(&json_str)
    }

    async fn get_pr(&self, number: u64) -> Result<crate::github::types::GhPullRequest> {
        let num_str = number.to_string();
        let json_str = self
            .run_gh(&["pr", "view", &num_str, "--json", PR_JSON_FIELDS])
            .await?;
        let prs = parse_prs_json(&format!("[{}]", json_str))?;
        prs.into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("PR #{} not found", number))
    }

    async fn submit_pr_review(
        &self,
        pr_number: u64,
        event: crate::github::types::PrReviewEvent,
        body: &str,
    ) -> Result<()> {
        let num_str = pr_number.to_string();
        let mut args = vec!["pr", "review", &num_str];
        let flag = format!("--{}", event.as_gh_arg());
        args.push(&flag);
        if !body.is_empty() {
            args.push("--body");
            args.push(body);
        }
        self.run_gh(&args).await?;
        Ok(())
    }

    async fn list_labels(&self) -> Result<Vec<String>> {
        let json_str = self
            .run_gh(&["label", "list", "--json", "name", "--limit", "200"])
            .await?;
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
        self.run_gh(&[
            "label",
            "create",
            name,
            "--color",
            color,
            "--description",
            "Managed by Maestro",
            "--force",
        ])
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    pub struct MockGitHubClient {
        inner: Arc<Mutex<MockState>>,
    }

    #[derive(Default)]
    struct MockState {
        issues: Vec<GhIssue>,
        milestones: Vec<GhMilestone>,
        add_label_error: Option<String>,
        remove_label_error: Option<String>,
        create_pr_response: Option<u64>,
        create_pr_error: Option<String>,
        get_issue_errors: std::collections::HashMap<u64, String>,
        list_prs_for_branch_responses: std::collections::HashMap<String, Vec<u64>>,

        add_label_calls: Vec<(u64, String)>,
        remove_label_calls: Vec<(u64, String)>,
        create_pr_calls: Vec<CreatePrCallRecord>,

        create_milestone_calls: Vec<(String, String)>,
        create_milestone_counter: u64,
        create_issue_calls: Vec<CreateIssueCallRecord>,
        create_issue_counter: u64,

        // Label management fields
        labels: Vec<String>,
        list_labels_calls: u32,
        list_labels_error: Option<String>,
        create_label_calls: Vec<(String, String)>,
        create_label_error: Option<String>,

        // PR review fields
        pull_requests: Vec<crate::github::types::GhPullRequest>,
        list_open_prs_error: Option<String>,
        get_pr_errors: std::collections::HashMap<u64, String>,
        submit_pr_review_error: Option<String>,
        submit_pr_review_calls: Vec<SubmitPrReviewCallRecord>,
    }

    #[derive(Debug, Clone)]
    pub struct SubmitPrReviewCallRecord {
        pub pr_number: u64,
        pub event: crate::github::types::PrReviewEvent,
        pub body: String,
    }

    #[derive(Debug, Clone)]
    pub struct CreatePrCallRecord {
        pub issue_number: u64,
        pub title: String,
        pub body: String,
        pub head_branch: String,
        pub base_branch: String,
    }

    #[derive(Debug, Clone)]
    pub struct CreateIssueCallRecord {
        pub title: String,
        pub body: String,
        pub labels: Vec<String>,
        pub milestone: Option<u64>,
    }

    impl MockGitHubClient {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set_issues(&self, issues: Vec<GhIssue>) {
            self.inner.lock().unwrap().issues = issues;
        }

        pub fn set_milestones(&self, milestones: Vec<GhMilestone>) {
            self.inner.lock().unwrap().milestones = milestones;
        }

        pub fn set_get_issue_error(&self, number: u64, msg: &str) {
            self.inner
                .lock()
                .unwrap()
                .get_issue_errors
                .insert(number, msg.to_string());
        }

        pub fn set_add_label_error(&self, msg: &str) {
            self.inner.lock().unwrap().add_label_error = Some(msg.to_string());
        }

        pub fn set_remove_label_error(&self, msg: &str) {
            self.inner.lock().unwrap().remove_label_error = Some(msg.to_string());
        }

        pub fn set_create_pr_response(&self, pr_number: u64) {
            self.inner.lock().unwrap().create_pr_response = Some(pr_number);
        }

        pub fn set_create_pr_error(&self, msg: &str) {
            self.inner.lock().unwrap().create_pr_error = Some(msg.to_string());
        }

        pub fn set_list_prs_for_branch(&self, branch: &str, pr_numbers: Vec<u64>) {
            self.inner
                .lock()
                .unwrap()
                .list_prs_for_branch_responses
                .insert(branch.to_string(), pr_numbers);
        }

        pub fn add_label_calls(&self) -> Vec<(u64, String)> {
            self.inner.lock().unwrap().add_label_calls.clone()
        }

        pub fn remove_label_calls(&self) -> Vec<(u64, String)> {
            self.inner.lock().unwrap().remove_label_calls.clone()
        }

        pub fn create_pr_calls(&self) -> Vec<CreatePrCallRecord> {
            self.inner.lock().unwrap().create_pr_calls.clone()
        }

        pub fn create_milestone_calls(&self) -> Vec<(String, String)> {
            self.inner.lock().unwrap().create_milestone_calls.clone()
        }

        pub fn create_issue_calls(&self) -> Vec<CreateIssueCallRecord> {
            self.inner.lock().unwrap().create_issue_calls.clone()
        }

        pub fn set_labels(&self, labels: Vec<String>) {
            self.inner.lock().unwrap().labels = labels;
        }

        pub fn set_list_labels_error(&self, msg: &str) {
            self.inner.lock().unwrap().list_labels_error = Some(msg.to_string());
        }

        pub fn set_create_label_error(&self, msg: &str) {
            self.inner.lock().unwrap().create_label_error = Some(msg.to_string());
        }

        pub fn list_labels_call_count(&self) -> u32 {
            self.inner.lock().unwrap().list_labels_calls
        }

        pub fn create_label_calls(&self) -> Vec<(String, String)> {
            self.inner.lock().unwrap().create_label_calls.clone()
        }

        pub fn set_pull_requests(&self, prs: Vec<crate::github::types::GhPullRequest>) {
            self.inner.lock().unwrap().pull_requests = prs;
        }

        pub fn set_list_open_prs_error(&self, msg: &str) {
            self.inner.lock().unwrap().list_open_prs_error = Some(msg.to_string());
        }

        pub fn set_get_pr_error(&self, number: u64, msg: &str) {
            self.inner
                .lock()
                .unwrap()
                .get_pr_errors
                .insert(number, msg.to_string());
        }

        pub fn set_submit_pr_review_error(&self, msg: &str) {
            self.inner.lock().unwrap().submit_pr_review_error = Some(msg.to_string());
        }

        pub fn submit_pr_review_calls(&self) -> Vec<SubmitPrReviewCallRecord> {
            self.inner.lock().unwrap().submit_pr_review_calls.clone()
        }
    }

    #[async_trait]
    impl GitHubClient for MockGitHubClient {
        async fn list_issues(&self, labels: &[&str]) -> Result<Vec<GhIssue>> {
            let state = self.inner.lock().unwrap();
            let label_set: std::collections::HashSet<&str> = labels.iter().copied().collect();
            let filtered = state
                .issues
                .iter()
                .filter(|i| {
                    label_set.is_empty() || i.labels.iter().any(|l| label_set.contains(l.as_str()))
                })
                .cloned()
                .collect();
            Ok(filtered)
        }

        async fn list_issues_by_milestone(&self, _milestone: &str) -> Result<Vec<GhIssue>> {
            let state = self.inner.lock().unwrap();
            Ok(state.issues.clone())
        }

        async fn list_milestones(&self, _state: &str) -> Result<Vec<GhMilestone>> {
            let state = self.inner.lock().unwrap();
            Ok(state.milestones.clone())
        }

        async fn get_issue(&self, number: u64) -> Result<GhIssue> {
            let state = self.inner.lock().unwrap();
            if let Some(err_msg) = state.get_issue_errors.get(&number) {
                anyhow::bail!("{}", err_msg);
            }
            state
                .issues
                .iter()
                .find(|i| i.number == number)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("mock: issue #{} not found", number))
        }

        async fn add_label(&self, issue: u64, label: &str) -> Result<()> {
            let mut state = self.inner.lock().unwrap();
            if let Some(ref err) = state.add_label_error {
                anyhow::bail!("{}", err);
            }
            state.add_label_calls.push((issue, label.to_string()));
            Ok(())
        }

        async fn remove_label(&self, issue: u64, label: &str) -> Result<()> {
            let mut state = self.inner.lock().unwrap();
            if let Some(ref err) = state.remove_label_error {
                anyhow::bail!("{}", err);
            }
            state.remove_label_calls.push((issue, label.to_string()));
            Ok(())
        }

        async fn create_pr(
            &self,
            issue_number: u64,
            title: &str,
            body: &str,
            head_branch: &str,
            base_branch: &str,
        ) -> Result<u64> {
            let mut state = self.inner.lock().unwrap();
            state.create_pr_calls.push(CreatePrCallRecord {
                issue_number,
                title: title.to_string(),
                body: body.to_string(),
                head_branch: head_branch.to_string(),
                base_branch: base_branch.to_string(),
            });
            if let Some(ref err) = state.create_pr_error {
                anyhow::bail!("{}", err);
            }
            Ok(state.create_pr_response.unwrap_or(1))
        }

        async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>> {
            let state = self.inner.lock().unwrap();
            Ok(state
                .list_prs_for_branch_responses
                .get(head_branch)
                .cloned()
                .unwrap_or_default())
        }

        async fn create_milestone(&self, title: &str, description: &str) -> Result<u64> {
            let mut state = self.inner.lock().unwrap();
            state.create_milestone_counter += 1;
            state
                .create_milestone_calls
                .push((title.to_string(), description.to_string()));
            Ok(state.create_milestone_counter)
        }

        async fn create_issue(
            &self,
            title: &str,
            body: &str,
            labels: &[String],
            milestone: Option<u64>,
        ) -> Result<u64> {
            let mut state = self.inner.lock().unwrap();
            state.create_issue_counter += 1;
            state.create_issue_calls.push(CreateIssueCallRecord {
                title: title.to_string(),
                body: body.to_string(),
                labels: labels.to_vec(),
                milestone,
            });
            Ok(state.create_issue_counter)
        }

        async fn list_open_prs(&self) -> Result<Vec<crate::github::types::GhPullRequest>> {
            let state = self.inner.lock().unwrap();
            if let Some(ref err) = state.list_open_prs_error {
                anyhow::bail!("{}", err);
            }
            Ok(state.pull_requests.clone())
        }

        async fn get_pr(&self, number: u64) -> Result<crate::github::types::GhPullRequest> {
            let state = self.inner.lock().unwrap();
            if let Some(err_msg) = state.get_pr_errors.get(&number) {
                anyhow::bail!("{}", err_msg);
            }
            state
                .pull_requests
                .iter()
                .find(|p| p.number == number)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("mock: PR #{} not found", number))
        }

        async fn submit_pr_review(
            &self,
            pr_number: u64,
            event: crate::github::types::PrReviewEvent,
            body: &str,
        ) -> Result<()> {
            let mut state = self.inner.lock().unwrap();
            if let Some(ref err) = state.submit_pr_review_error {
                anyhow::bail!("{}", err);
            }
            state.submit_pr_review_calls.push(SubmitPrReviewCallRecord {
                pr_number,
                event,
                body: body.to_string(),
            });
            Ok(())
        }

        async fn list_labels(&self) -> Result<Vec<String>> {
            let mut state = self.inner.lock().unwrap();
            state.list_labels_calls += 1;
            if let Some(ref err) = state.list_labels_error {
                anyhow::bail!("{}", err);
            }
            Ok(state.labels.clone())
        }

        async fn create_label(&self, name: &str, color: &str) -> Result<()> {
            let mut state = self.inner.lock().unwrap();
            if let Some(ref err) = state.create_label_error {
                anyhow::bail!("{}", err);
            }
            state
                .create_label_calls
                .push((name.to_string(), color.to_string()));
            if !state.labels.contains(&name.to_string()) {
                state.labels.push(name.to_string());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::MockGitHubClient;

    fn make_issue(number: u64, labels: &[&str]) -> GhIssue {
        GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }

    // parse_issues_json

    #[test]
    fn parse_issues_json_valid_array() {
        let json = r#"[
            {"number": 1, "title": "First", "body": "desc", "labels": [{"name": "maestro:ready"}], "state": "open", "url": "https://github.com/r/i/1"},
            {"number": 2, "title": "Second", "body": "", "labels": [], "state": "open", "url": "https://github.com/r/i/2"}
        ]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 1);
        assert_eq!(issues[0].labels, vec!["maestro:ready"]);
        assert_eq!(issues[1].number, 2);
    }

    #[test]
    fn parse_issues_json_empty_array() {
        let issues = parse_issues_json("[]").unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn parse_issues_json_invalid_json_returns_err() {
        assert!(parse_issues_json("{not json}").is_err());
    }

    #[test]
    fn parse_issues_json_normalizes_state_to_lowercase() {
        let json = r#"[
            {"number": 1, "title": "T", "body": "", "state": "OPEN", "url": "u", "labels": []},
            {"number": 2, "title": "T2", "body": "", "state": "CLOSED", "url": "u2", "labels": []}
        ]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(
            issues[0].state, "open",
            "OPEN must be normalized to lowercase"
        );
        assert_eq!(
            issues[1].state, "closed",
            "CLOSED must be normalized to lowercase"
        );
    }

    #[test]
    fn parse_issues_json_extracts_label_names_from_objects() {
        let json = r#"[
            {"number": 5, "title": "T", "body": "", "state": "open", "url": "u",
             "labels": [{"name": "priority:P0"}, {"name": "maestro:ready"}]}
        ]"#;
        let issues = parse_issues_json(json).unwrap();
        assert_eq!(issues[0].labels, vec!["priority:P0", "maestro:ready"]);
    }

    // MockGitHubClient tests

    #[tokio::test]
    async fn mock_list_issues_returns_all_when_no_filter() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            make_issue(1, &["maestro:ready"]),
            make_issue(2, &["bug"]),
        ]);
        let issues = client.list_issues(&[]).await.unwrap();
        assert_eq!(issues.len(), 2);
    }

    #[tokio::test]
    async fn mock_list_issues_filters_by_label() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            make_issue(1, &["maestro:ready"]),
            make_issue(2, &["bug"]),
        ]);
        let issues = client.list_issues(&["maestro:ready"]).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 1);
    }

    #[tokio::test]
    async fn mock_get_issue_found() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![make_issue(42, &["maestro:ready"])]);
        let issue = client.get_issue(42).await.unwrap();
        assert_eq!(issue.number, 42);
    }

    #[tokio::test]
    async fn mock_get_issue_not_found_returns_err() {
        let client = MockGitHubClient::new();
        let result = client.get_issue(999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mock_get_issue_custom_error() {
        let client = MockGitHubClient::new();
        client.set_get_issue_error(10, "rate limited");
        client.set_issues(vec![make_issue(10, &[])]);
        let err = client.get_issue(10).await.unwrap_err();
        assert!(err.to_string().contains("rate limited"));
    }

    #[tokio::test]
    async fn mock_add_label_records_call() {
        let client = MockGitHubClient::new();
        client.add_label(7, "maestro:in-progress").await.unwrap();
        let calls = client.add_label_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (7, "maestro:in-progress".to_string()));
    }

    #[tokio::test]
    async fn mock_add_label_propagates_configured_error() {
        let client = MockGitHubClient::new();
        client.set_add_label_error("label not found");
        let result = client.add_label(1, "anything").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("label not found"));
    }

    #[tokio::test]
    async fn mock_remove_label_records_call() {
        let client = MockGitHubClient::new();
        client.remove_label(5, "maestro:ready").await.unwrap();
        let calls = client.remove_label_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (5, "maestro:ready".to_string()));
    }

    #[tokio::test]
    async fn mock_remove_label_propagates_configured_error() {
        let client = MockGitHubClient::new();
        client.set_remove_label_error("network error");
        let result = client.remove_label(1, "anything").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mock_create_pr_records_call() {
        let client = MockGitHubClient::new();
        client.set_create_pr_response(42);
        let pr_number = client
            .create_pr(
                10,
                "feat: add thing",
                "Closes #10",
                "maestro/issue-10",
                "main",
            )
            .await
            .unwrap();
        assert_eq!(pr_number, 42);

        let calls = client.create_pr_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].issue_number, 10);
        assert_eq!(calls[0].head_branch, "maestro/issue-10");
        assert_eq!(calls[0].base_branch, "main");
    }

    #[tokio::test]
    async fn mock_create_pr_propagates_configured_error() {
        let client = MockGitHubClient::new();
        client.set_create_pr_error("branch not found");
        let result = client
            .create_pr(1, "title", "body", "bad-branch", "main")
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("branch not found"));
    }

    // parse_milestones_json

    #[test]
    fn parse_milestones_json_valid_array() {
        let json = r#"[
            {"number": 1, "title": "v1.0", "description": "First release", "state": "open", "open_issues": 3, "closed_issues": 7},
            {"number": 2, "title": "v2.0", "description": "", "state": "open", "open_issues": 0, "closed_issues": 0}
        ]"#;
        let milestones = parse_milestones_json(json).unwrap();
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[0].number, 1);
        assert_eq!(milestones[0].title, "v1.0");
        assert_eq!(milestones[0].open_issues, 3);
        assert_eq!(milestones[0].closed_issues, 7);
        assert_eq!(milestones[1].number, 2);
    }

    #[test]
    fn parse_milestones_json_empty_array() {
        let milestones = parse_milestones_json("[]").unwrap();
        assert!(milestones.is_empty());
    }

    #[test]
    fn parse_milestones_json_invalid_json_returns_err() {
        assert!(parse_milestones_json("{not json}").is_err());
    }

    #[test]
    fn parse_milestones_json_missing_optional_fields_default() {
        let json = r#"[{"number": 5, "title": "v5", "state": "open"}]"#;
        let milestones = parse_milestones_json(json).unwrap();
        assert_eq!(milestones[0].description, "");
        assert_eq!(milestones[0].open_issues, 0);
        assert_eq!(milestones[0].closed_issues, 0);
    }

    // MockGitHubClient::list_milestones

    #[tokio::test]
    async fn mock_list_milestones_returns_stored_milestones() {
        let client = MockGitHubClient::new();
        client.set_milestones(vec![
            GhMilestone {
                number: 1,
                title: "v1.0".to_string(),
                description: String::new(),
                state: "open".to_string(),
                open_issues: 2,
                closed_issues: 3,
            },
            GhMilestone {
                number: 2,
                title: "v2.0".to_string(),
                description: String::new(),
                state: "open".to_string(),
                open_issues: 0,
                closed_issues: 0,
            },
        ]);
        let milestones = client.list_milestones("open").await.unwrap();
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[0].title, "v1.0");
    }

    #[tokio::test]
    async fn mock_list_milestones_returns_empty_when_none_set() {
        let client = MockGitHubClient::new();
        let milestones = client.list_milestones("open").await.unwrap();
        assert!(milestones.is_empty());
    }

    // -- list_prs_for_branch --

    #[tokio::test]
    async fn mock_list_prs_for_branch_returns_configured_prs() {
        let client = MockGitHubClient::new();
        client.set_list_prs_for_branch("maestro/issue-42", vec![10, 20]);
        let prs = client
            .list_prs_for_branch("maestro/issue-42")
            .await
            .unwrap();
        assert_eq!(prs, vec![10, 20]);
    }

    #[tokio::test]
    async fn mock_list_prs_for_branch_returns_empty_for_unknown_branch() {
        let client = MockGitHubClient::new();
        let prs = client
            .list_prs_for_branch("maestro/issue-99")
            .await
            .unwrap();
        assert!(prs.is_empty());
    }

    // -- is_auth_error --

    #[test]
    fn is_auth_error_returns_true_for_not_logged_in() {
        assert!(is_auth_error("ERROR: not logged in to any GitHub host"));
    }

    #[test]
    fn is_auth_error_returns_true_for_authentication_required() {
        assert!(is_auth_error("gh: authentication required"));
    }

    #[test]
    fn is_auth_error_returns_true_for_http_401() {
        assert!(is_auth_error("HTTP 401: Unauthorized"));
    }

    #[test]
    fn is_auth_error_returns_true_for_auth_token_errors() {
        assert!(is_auth_error(
            "error refreshing authentication token: token expired"
        ));
    }

    #[test]
    fn is_auth_error_returns_true_for_try_authenticating() {
        assert!(is_auth_error("try authenticating with: gh auth login"));
    }

    #[test]
    fn is_auth_error_returns_false_for_network_timeout() {
        assert!(!is_auth_error("dial tcp: connection timed out"));
    }

    #[test]
    fn is_auth_error_returns_false_for_branch_not_found() {
        assert!(!is_auth_error("ERROR: branch 'maestro/issue-99' not found"));
    }

    #[test]
    fn is_auth_error_returns_false_for_empty_string() {
        assert!(!is_auth_error(""));
    }

    #[test]
    fn is_auth_error_is_case_insensitive() {
        assert!(is_auth_error("NOT LOGGED IN TO ANY GITHUB HOST"));
        assert!(is_auth_error("Http 401: unauthorized"));
        assert!(is_auth_error("AUTHENTICATION REQUIRED"));
    }

    // -- is_gh_auth_error --

    #[test]
    fn is_gh_auth_error_returns_true_for_sentinel() {
        let err = anyhow::anyhow!("[gh-auth-error] not logged in");
        assert!(is_gh_auth_error(&err));
    }

    #[test]
    fn is_gh_auth_error_returns_false_for_regular_error() {
        let err = anyhow::anyhow!("gh command failed: branch not found");
        assert!(!is_gh_auth_error(&err));
    }

    // -- create_milestone / create_issue mock tests --

    #[tokio::test]
    async fn mock_create_milestone_records_call_and_returns_number() {
        let client = MockGitHubClient::new();
        let n1 = client
            .create_milestone("M0", "First milestone")
            .await
            .unwrap();
        let n2 = client
            .create_milestone("M1", "Second milestone")
            .await
            .unwrap();
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
        let calls = client.create_milestone_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "M0");
        assert_eq!(calls[1].0, "M1");
    }

    #[tokio::test]
    async fn mock_create_issue_records_call_and_returns_number() {
        let client = MockGitHubClient::new();
        let n = client
            .create_issue("feat: thing", "body", &["enhancement".into()], Some(1))
            .await
            .unwrap();
        assert_eq!(n, 1);
        let calls = client.create_issue_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].title, "feat: thing");
        assert_eq!(calls[0].labels, vec!["enhancement"]);
        assert_eq!(calls[0].milestone, Some(1));
    }

    #[tokio::test]
    async fn mock_create_issue_increments_counter() {
        let client = MockGitHubClient::new();
        let n1 = client.create_issue("a", "", &[], None).await.unwrap();
        let n2 = client.create_issue("b", "", &[], None).await.unwrap();
        let n3 = client.create_issue("c", "", &[], None).await.unwrap();
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
        assert_eq!(n3, 3);
    }

    // -- PR review mock tests --

    fn make_pr(number: u64) -> crate::github::types::GhPullRequest {
        crate::github::types::GhPullRequest {
            number,
            title: format!("PR #{}", number),
            body: String::new(),
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/pull/{}", number),
            head_branch: format!("fix/issue-{}", number),
            base_branch: "main".to_string(),
            author: "bot".to_string(),
            labels: vec![],
            draft: false,
            mergeable: true,
            additions: 0,
            deletions: 0,
            changed_files: 0,
        }
    }

    #[tokio::test]
    async fn mock_list_open_prs_returns_configured_prs() {
        let client = MockGitHubClient::new();
        client.set_pull_requests(vec![make_pr(10), make_pr(11)]);
        let prs = client.list_open_prs().await.unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].number, 10);
        assert_eq!(prs[1].number, 11);
    }

    #[tokio::test]
    async fn mock_list_open_prs_returns_empty_by_default() {
        let client = MockGitHubClient::new();
        let prs = client.list_open_prs().await.unwrap();
        assert!(prs.is_empty());
    }

    #[tokio::test]
    async fn mock_list_open_prs_propagates_configured_error() {
        let client = MockGitHubClient::new();
        client.set_list_open_prs_error("connection refused");
        let result = client.list_open_prs().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("connection refused")
        );
    }

    #[tokio::test]
    async fn mock_get_pr_returns_pr_by_number() {
        let client = MockGitHubClient::new();
        client.set_pull_requests(vec![make_pr(42)]);
        let pr = client.get_pr(42).await.unwrap();
        assert_eq!(pr.number, 42);
    }

    #[tokio::test]
    async fn mock_get_pr_returns_not_found_for_missing_number() {
        let client = MockGitHubClient::new();
        let result = client.get_pr(99).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mock_get_pr_propagates_configured_error() {
        let client = MockGitHubClient::new();
        client.set_get_pr_error(5, "rate limited");
        client.set_pull_requests(vec![make_pr(5)]);
        let result = client.get_pr(5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("rate limited"));
    }

    #[tokio::test]
    async fn mock_submit_pr_review_records_approve_call() {
        use crate::github::types::PrReviewEvent;
        let client = MockGitHubClient::new();
        client
            .submit_pr_review(7, PrReviewEvent::Approve, "LGTM")
            .await
            .unwrap();
        let calls = client.submit_pr_review_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].pr_number, 7);
        assert_eq!(calls[0].event, PrReviewEvent::Approve);
        assert_eq!(calls[0].body, "LGTM");
    }

    #[tokio::test]
    async fn mock_submit_pr_review_records_request_changes_call() {
        use crate::github::types::PrReviewEvent;
        let client = MockGitHubClient::new();
        client
            .submit_pr_review(3, PrReviewEvent::RequestChanges, "needs work")
            .await
            .unwrap();
        let calls = client.submit_pr_review_calls();
        assert_eq!(calls[0].event, PrReviewEvent::RequestChanges);
    }

    #[tokio::test]
    async fn mock_submit_pr_review_records_comment_call() {
        use crate::github::types::PrReviewEvent;
        let client = MockGitHubClient::new();
        client
            .submit_pr_review(1, PrReviewEvent::Comment, "nice")
            .await
            .unwrap();
        let calls = client.submit_pr_review_calls();
        assert_eq!(calls[0].event, PrReviewEvent::Comment);
    }

    #[tokio::test]
    async fn mock_submit_pr_review_propagates_configured_error() {
        use crate::github::types::PrReviewEvent;
        let client = MockGitHubClient::new();
        client.set_submit_pr_review_error("forbidden");
        let result = client.submit_pr_review(1, PrReviewEvent::Approve, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("forbidden"));
    }

    // -- parse_prs_json --

    #[test]
    fn parse_prs_json_valid_array() {
        let json = r#"[
            {"number": 1, "title": "Fix bug", "body": "desc", "state": "OPEN", "url": "https://github.com/r/p/1",
             "headRefName": "fix/bug", "baseRefName": "main", "author": {"login": "user1"},
             "labels": [{"name": "enhancement"}], "isDraft": false, "mergeable": "MERGEABLE",
             "additions": 10, "deletions": 5, "changedFiles": 3}
        ]"#;
        let prs = parse_prs_json(json).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 1);
        assert_eq!(prs[0].title, "Fix bug");
        assert_eq!(prs[0].head_branch, "fix/bug");
        assert_eq!(prs[0].base_branch, "main");
        assert_eq!(prs[0].author, "user1");
        assert_eq!(prs[0].state, "open");
        assert!(prs[0].mergeable);
        assert_eq!(prs[0].additions, 10);
        assert_eq!(prs[0].labels, vec!["enhancement"]);
    }

    #[test]
    fn parse_prs_json_empty_array() {
        let prs = parse_prs_json("[]").unwrap();
        assert!(prs.is_empty());
    }

    #[test]
    fn parse_prs_json_invalid_json_returns_err() {
        assert!(parse_prs_json("{not json}").is_err());
    }
}
