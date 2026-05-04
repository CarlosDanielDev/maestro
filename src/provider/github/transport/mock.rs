use super::{CreateOutcome, GhIssue, GhMilestone};
use crate::provider::github::types::{GhPullRequest, PrReviewEvent};
use std::sync::{Arc, Mutex};

mod impl_client;
#[cfg(test)]
mod tests_create;
#[cfg(test)]
mod tests_issue;
#[cfg(test)]
mod tests_pr;

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
    pull_requests: Vec<GhPullRequest>,
    list_open_prs_error: Option<String>,
    get_pr_errors: std::collections::HashMap<u64, String>,
    submit_pr_review_error: Option<String>,
    submit_pr_review_calls: Vec<SubmitPrReviewCallRecord>,

    // Milestone health-check fields (#500)
    list_issues_by_milestone_error: Option<String>,
    patch_milestone_error: Option<String>,
    patch_milestone_calls: Vec<(u64, String)>,
}

#[derive(Debug, Clone)]
pub struct SubmitPrReviewCallRecord {
    pub pr_number: u64,
    pub event: PrReviewEvent,
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
        let mut guard = self.inner.lock().unwrap();
        let max = issues.iter().map(|i| i.number).max().unwrap_or(0);
        guard.create_issue_counter = guard.create_issue_counter.max(max);
        guard.issues = issues;
    }

    pub fn set_milestones(&self, milestones: Vec<GhMilestone>) {
        let mut guard = self.inner.lock().unwrap();
        let max = milestones.iter().map(|m| m.number).max().unwrap_or(0);
        guard.create_milestone_counter = guard.create_milestone_counter.max(max);
        guard.milestones = milestones;
    }

    /// Alias of `set_milestones` matching the naming requested in #453.
    pub fn set_existing_milestones(&self, milestones: Vec<GhMilestone>) {
        self.set_milestones(milestones);
    }

    /// Alias of `set_issues` matching the naming requested in #453.
    pub fn set_existing_issues(&self, issues: Vec<GhIssue>) {
        self.set_issues(issues);
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

    pub fn set_pull_requests(&self, prs: Vec<GhPullRequest>) {
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

    // Milestone health-check helpers (#500)

    pub fn set_list_issues_by_milestone_error(&self, msg: &str) {
        self.inner.lock().unwrap().list_issues_by_milestone_error = Some(msg.to_string());
    }

    pub fn set_patch_milestone_error(&self, msg: &str) {
        self.inner.lock().unwrap().patch_milestone_error = Some(msg.to_string());
    }

    pub fn clear_patch_milestone_error(&self) {
        self.inner.lock().unwrap().patch_milestone_error = None;
    }

    pub fn patch_milestone_calls(&self) -> Vec<(u64, String)> {
        self.inner.lock().unwrap().patch_milestone_calls.clone()
    }
}
