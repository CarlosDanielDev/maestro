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
}

/// Parse JSON output from `gh issue list --json ...`.
pub fn parse_issues_json(json_str: &str) -> Result<Vec<GhIssue>> {
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(json_str).context("Failed to parse GitHub issues JSON")?;
    let mut issues = Vec::new();
    for v in raw {
        let labels: Vec<String> = v
            .get("labels")
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
            .unwrap_or_default();

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
                .to_string(),
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
}

/// Validate user-provided strings before passing to `gh` CLI.
/// Prevents argument injection (values starting with `-`).
fn validate_gh_arg(value: &str, field_name: &str) -> Result<()> {
    if value.starts_with('-') {
        anyhow::bail!("{} must not start with '-' (got {:?})", field_name, value);
    }
    if value.contains('\0') {
        anyhow::bail!("{} must not contain null bytes", field_name);
    }
    Ok(())
}

/// Implementation that shells out to `gh` CLI.
pub struct GhCliClient;

impl GhCliClient {
    pub fn new() -> Self {
        Self
    }

    async fn run_gh(&self, args: &[&str]) -> Result<String> {
        let output = tokio::process::Command::new("gh")
            .args(args)
            .output()
            .await
            .context("Failed to run `gh` CLI. Is it installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
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
            "number,title,body,labels,state,url",
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
                "number,title,body,labels,state,url",
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
                    "maestro:in-progress" => "FBCA04",
                    "maestro:done" => "1D76DB",
                    "maestro:failed" => "D93F0B",
                    _ => "EDEDED",
                };
                // Create the label (ignore errors if it already exists)
                let _ = self
                    .run_gh(&[
                        "label",
                        "create",
                        label,
                        "--color",
                        color,
                        "--description",
                        "Managed by Maestro",
                        "--force",
                    ])
                    .await;
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

        add_label_calls: Vec<(u64, String)>,
        remove_label_calls: Vec<(u64, String)>,
        create_pr_calls: Vec<CreatePrCallRecord>,
    }

    #[derive(Debug, Clone)]
    pub struct CreatePrCallRecord {
        pub issue_number: u64,
        pub title: String,
        pub body: String,
        pub head_branch: String,
        pub base_branch: String,
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

        pub fn add_label_calls(&self) -> Vec<(u64, String)> {
            self.inner.lock().unwrap().add_label_calls.clone()
        }

        pub fn remove_label_calls(&self) -> Vec<(u64, String)> {
            self.inner.lock().unwrap().remove_label_calls.clone()
        }

        pub fn create_pr_calls(&self) -> Vec<CreatePrCallRecord> {
            self.inner.lock().unwrap().create_pr_calls.clone()
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
}
