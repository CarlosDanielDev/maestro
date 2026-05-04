use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static BLOCKED_BY_RE: OnceLock<regex::Regex> = OnceLock::new();

fn blocked_by_regex() -> &'static regex::Regex {
    BLOCKED_BY_RE.get_or_init(|| regex::Regex::new(r"(?i)blocked-by:\s*#(\d+)").unwrap())
}

/// Which code hosting provider is configured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Github,
    /// Experimental until v0.24.0; requires `experimental.azure_devops = true`.
    AzureDevops,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Priority {
    P0 = 0,
    P1 = 1,
    #[default]
    P2 = 2,
}

impl Priority {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "priority:P0" => Some(Self::P0),
            "priority:P1" => Some(Self::P1),
            "priority:P2" => Some(Self::P2),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaestroLabel {
    Ready,
    InProgress,
    Done,
    Failed,
}

impl MaestroLabel {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "maestro:ready" => Some(Self::Ready),
            "maestro:in-progress" => Some(Self::InProgress),
            "maestro:done" => Some(Self::Done),
            "maestro:failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "maestro:ready",
            Self::InProgress => "maestro:in-progress",
            Self::Done => "maestro:done",
            Self::Failed => "maestro:failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Orchestrator,
    Vibe,
}

impl SessionMode {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "mode:orchestrator" => Some(Self::Orchestrator),
            "mode:vibe" => Some(Self::Vibe),
            _ => None,
        }
    }

    pub fn as_config_str(&self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::Vibe => "vibe",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub state: String,
    pub html_url: String,
    #[serde(default)]
    pub milestone: Option<u64>,
    #[serde(default)]
    pub assignees: Vec<String>,
}

impl Issue {
    /// Extract the priority from labels. Defaults to P2.
    pub fn priority(&self) -> Priority {
        self.labels
            .iter()
            .find_map(|l| Priority::from_label(l))
            .unwrap_or_default()
    }

    /// Extract the maestro session mode from labels.
    pub fn session_mode(&self) -> Option<SessionMode> {
        self.labels.iter().find_map(|l| SessionMode::from_label(l))
    }

    /// Extract `blocked-by:#N` dependencies from labels.
    pub fn blocked_by_from_labels(&self) -> Vec<u64> {
        self.labels
            .iter()
            .filter_map(|l| {
                l.strip_prefix("blocked-by:#")
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .collect()
    }

    /// Extract `blocked-by: #N` from issue body text (case-insensitive).
    pub fn blocked_by_from_body(&self) -> Vec<u64> {
        blocked_by_regex()
            .captures_iter(&self.body)
            .filter_map(|cap| cap[1].parse::<u64>().ok())
            .collect()
    }

    /// All blocking issue numbers (union of labels + body, deduplicated).
    pub fn all_blockers(&self) -> Vec<u64> {
        let mut blockers = self.blocked_by_from_labels();
        blockers.extend(self.blocked_by_from_body());
        blockers.sort_unstable();
        blockers.dedup();
        blockers
    }

    #[allow(dead_code)]
    pub fn has_maestro_label(&self, label: MaestroLabel) -> bool {
        self.labels.iter().any(|l| l == label.as_str())
    }

    /// Build a prompt for an unattended Claude session working on this issue.
    pub fn unattended_prompt(&self) -> String {
        format!(
            "Work on GitHub issue #{}.\n\nTitle: {}\n\nDescription:\n{}\n\n\
             IMPORTANT: You are running in unattended mode (no human at the terminal). \
             Do NOT use AskUserQuestion or ask for clarification — make your best judgment \
             and proceed autonomously. Read relevant source files first, then implement \
             the required changes.",
            self.number, self.title, self.body
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub state: String,
    #[serde(default)]
    pub open_issues: u32,
    #[serde(default)]
    pub closed_issues: u32,
}

/// A pull request from a repository provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub html_url: String,
    pub head_branch: String,
    pub base_branch: String,
    pub author: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub mergeable: bool,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
}

/// The type of review action to submit on a pull request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    #[default]
    Comment,
}

impl ReviewEvent {
    pub fn as_gh_arg(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::RequestChanges => "request-changes",
            Self::Comment => "comment",
        }
    }

    #[allow(dead_code)] // Reason: used by PR review screen rendering
    pub fn label(&self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::RequestChanges => "Request Changes",
            Self::Comment => "Comment",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Comment => Self::Approve,
            Self::Approve => Self::RequestChanges,
            Self::RequestChanges => Self::Comment,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Comment => Self::RequestChanges,
            Self::Approve => Self::Comment,
            Self::RequestChanges => Self::Approve,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_default_is_github() {
        assert_eq!(ProviderKind::default(), ProviderKind::Github);
    }

    #[test]
    fn provider_kind_serializes_github() {
        let s = serde_json::to_string(&ProviderKind::Github).unwrap();
        assert_eq!(s, "\"github\"");
    }

    #[test]
    fn provider_kind_serializes_azure_devops() {
        let s = serde_json::to_string(&ProviderKind::AzureDevops).unwrap();
        assert_eq!(s, "\"azure_devops\"");
    }

    #[test]
    fn provider_kind_deserializes_github() {
        let k: ProviderKind = serde_json::from_str("\"github\"").unwrap();
        assert_eq!(k, ProviderKind::Github);
    }

    #[test]
    fn provider_kind_deserializes_azure_devops() {
        let k: ProviderKind = serde_json::from_str("\"azure_devops\"").unwrap();
        assert_eq!(k, ProviderKind::AzureDevops);
    }

    #[test]
    fn provider_kind_unknown_returns_err() {
        assert!(serde_json::from_str::<ProviderKind>("\"gitlab\"").is_err());
    }
}
