use crate::provider::types::{PullRequest, ReviewEvent};
use anyhow::{Context, Result};

pub(super) fn parse_prs_json(json_str: &str) -> Result<Vec<PullRequest>> {
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(json_str).context("Failed to parse Azure DevOps PRs JSON")?;
    Ok(raw.iter().map(parse_pr_value).collect())
}

pub(super) fn parse_pr_json(json_str: &str) -> Result<PullRequest> {
    let raw: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse Azure DevOps PR JSON")?;
    Ok(parse_pr_value(&raw))
}

pub(super) fn review_event_vote(event: ReviewEvent) -> Option<&'static str> {
    match event {
        ReviewEvent::Approve => Some("approve"),
        ReviewEvent::RequestChanges => Some("reject"),
        ReviewEvent::Comment => None,
    }
}

fn parse_pr_value(v: &serde_json::Value) -> PullRequest {
    PullRequest {
        number: v
            .get("pullRequestId")
            .and_then(|number| number.as_u64())
            .unwrap_or(0),
        title: string_field(v, "title"),
        body: string_field(v, "description"),
        state: string_field(v, "status").to_lowercase(),
        html_url: string_field(v, "url"),
        head_branch: string_field(v, "sourceRefName"),
        base_branch: string_field(v, "targetRefName"),
        author: v
            .get("createdBy")
            .and_then(|created_by| created_by.get("uniqueName"))
            .and_then(|unique_name| unique_name.as_str())
            .unwrap_or("")
            .to_string(),
        labels: Vec::new(),
        draft: false,
        mergeable: false,
        additions: 0,
        deletions: 0,
        changed_files: 0,
    }
}

fn string_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string()
}
