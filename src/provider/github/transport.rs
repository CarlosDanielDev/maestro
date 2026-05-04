use super::types::{GhIssue, GhMilestone, GhPullRequest, PrReviewEvent};
use anyhow::Result;
use async_trait::async_trait;

mod cli;
mod cli_impl;
mod errors;
mod parsing;

#[cfg(test)]
pub mod mock;

#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod errors_tests;

pub use cli::GhCliClient;
pub(crate) use errors::parse_pr_number_from_create_output;
pub use errors::{is_auth_error, is_gh_auth_error, redact_secrets};
pub use parsing::{parse_issues_json, parse_milestones_json, parse_prs_json};

use cli::argv_refs;
use errors::{
    GH_AUTH_ERROR_SENTINEL, is_label_not_found_error, normalize_paginated_json_arrays,
    with_rate_limit_retries,
};

/// JSON fields requested from `gh pr list/view`.
pub(super) const PR_JSON_FIELDS: &str = "number,title,body,state,url,headRefName,baseRefName,author,labels,isDraft,mergeable,additions,deletions,changedFiles";

/// Outcome of a `create_milestone` / `create_issue` call. Tells callers
/// whether a new artifact was freshly created or an existing one was
/// matched (by equivalent title, open or closed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOutcome {
    /// A new milestone/issue was created. The `u64` is its number.
    Created(u64),
    /// An existing milestone/issue with an equivalent title was found.
    /// `state` is the current state (`"open"` or `"closed"`).
    Existed { number: u64, state: String },
}

impl CreateOutcome {
    /// Convenience: the number regardless of whether it was newly created
    /// or matched an existing record.
    pub fn number(&self) -> u64 {
        match self {
            Self::Created(n) => *n,
            Self::Existed { number, .. } => *number,
        }
    }

    /// Whether this outcome reused an existing record.
    pub fn is_existed(&self) -> bool {
        matches!(self, Self::Existed { .. })
    }
}

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
    #[allow(dead_code)] // Reason: orphan branch detection feature
    async fn list_prs_for_branch(&self, head_branch: &str) -> Result<Vec<u64>>;
    async fn create_milestone(&self, title: &str, description: &str) -> Result<CreateOutcome>;
    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<CreateOutcome>;
    async fn list_open_prs(&self) -> Result<Vec<GhPullRequest>>;
    #[allow(dead_code)] // Reason: PR detail view — currently PR data comes from list
    async fn get_pr(&self, number: u64) -> Result<GhPullRequest>;
    async fn submit_pr_review(
        &self,
        pr_number: u64,
        event: PrReviewEvent,
        body: &str,
    ) -> Result<()>;
    async fn list_labels(&self) -> Result<Vec<String>>;
    async fn create_label(&self, name: &str, color: &str) -> Result<()>;
    async fn patch_milestone_description(
        &self,
        milestone_number: u64,
        description: &str,
    ) -> Result<()>;
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
    async fn create_milestone(&self, title: &str, description: &str) -> Result<CreateOutcome> {
        (**self).create_milestone(title, description).await
    }
    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
        milestone: Option<u64>,
    ) -> Result<CreateOutcome> {
        (**self).create_issue(title, body, labels, milestone).await
    }
    async fn list_open_prs(&self) -> Result<Vec<GhPullRequest>> {
        (**self).list_open_prs().await
    }
    async fn get_pr(&self, number: u64) -> Result<GhPullRequest> {
        (**self).get_pr(number).await
    }
    async fn submit_pr_review(
        &self,
        pr_number: u64,
        event: PrReviewEvent,
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
    async fn patch_milestone_description(
        &self,
        milestone_number: u64,
        description: &str,
    ) -> Result<()> {
        (**self)
            .patch_milestone_description(milestone_number, description)
            .await
    }
}
