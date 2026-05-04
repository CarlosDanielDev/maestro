use super::*;
use crate::provider::github::transport::GitHubClient;
use anyhow::Result;
use async_trait::async_trait;

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
        if let Some(ref err) = state.list_issues_by_milestone_error {
            anyhow::bail!("{}", err);
        }
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

    async fn create_milestone(&self, title: &str, description: &str) -> Result<CreateOutcome> {
        use crate::util::{titles_equivalent, validate_title};
        let normalized = validate_title(title, "milestone title")?;

        let mut state = self.inner.lock().unwrap();
        let outcome = if let Some(existing) = state
            .milestones
            .iter()
            .find(|m| titles_equivalent(&m.title, &normalized))
        {
            CreateOutcome::Existed {
                number: existing.number,
                state: existing.state.clone(),
            }
        } else {
            state.create_milestone_counter += 1;
            let number = state.create_milestone_counter;
            state
                .create_milestone_calls
                .push((normalized.clone(), description.to_string()));
            state.milestones.push(GhMilestone {
                number,
                title: normalized,
                description: description.to_string(),
                state: "open".to_string(),
                open_issues: 0,
                closed_issues: 0,
            });
            CreateOutcome::Created(number)
        };
        drop(state);
        Ok(outcome)
    }

    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<CreateOutcome> {
        use crate::util::{titles_equivalent, validate_title};
        let normalized = validate_title(title, "issue title")?;

        let mut state = self.inner.lock().unwrap();
        let outcome = if let Some(existing) = state
            .issues
            .iter()
            .find(|i| titles_equivalent(&i.title, &normalized))
        {
            CreateOutcome::Existed {
                number: existing.number,
                state: existing.state.clone(),
            }
        } else {
            state.create_issue_counter += 1;
            let number = state.create_issue_counter;
            state.create_issue_calls.push(CreateIssueCallRecord {
                title: normalized.clone(),
                body: body.to_string(),
                labels: labels.to_vec(),
                milestone,
            });
            state.issues.push(GhIssue {
                number,
                title: normalized,
                body: body.to_string(),
                labels: labels.to_vec(),
                state: "open".to_string(),
                html_url: format!("https://github.com/mock/repo/issues/{}", number),
                milestone,
                assignees: Vec::new(),
            });
            CreateOutcome::Created(number)
        };
        drop(state);
        Ok(outcome)
    }

    async fn list_open_prs(&self) -> Result<Vec<GhPullRequest>> {
        let state = self.inner.lock().unwrap();
        if let Some(ref err) = state.list_open_prs_error {
            anyhow::bail!("{}", err);
        }
        Ok(state.pull_requests.clone())
    }

    async fn get_pr(&self, number: u64) -> Result<GhPullRequest> {
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
        event: PrReviewEvent,
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

    async fn patch_milestone_description(
        &self,
        milestone_number: u64,
        description: &str,
    ) -> Result<()> {
        let mut state = self.inner.lock().unwrap();
        state
            .patch_milestone_calls
            .push((milestone_number, description.to_string()));
        if let Some(ref err) = state.patch_milestone_error {
            anyhow::bail!("{}", err);
        }
        // Mirror the production write into the in-memory milestone, so
        // follow-up `list_milestones` calls see the new description.
        if let Some(m) = state
            .milestones
            .iter_mut()
            .find(|m| m.number == milestone_number)
        {
            m.description = description.to_string();
        }
        Ok(())
    }
}
