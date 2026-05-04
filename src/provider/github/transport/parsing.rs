use super::GhIssue;
use crate::provider::github::types::GhMilestone;
use anyhow::{Context, Result};

/// Extract label names from a JSON value containing `{"labels": [{"name": "..."}, ...]}`.
pub(super) fn extract_label_names(v: &serde_json::Value) -> Vec<String> {
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
pub fn parse_prs_json(
    json_str: &str,
) -> Result<Vec<crate::provider::github::types::GhPullRequest>> {
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

        prs.push(crate::provider::github::types::GhPullRequest {
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
