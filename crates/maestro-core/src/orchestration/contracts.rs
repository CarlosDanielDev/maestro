//! Subagent output contracts. L1 enforces; L2 trusts.
//! See spec §4 "Role output contracts" for derivation.

#![allow(dead_code)]

use crate::agent_provider::types::AgentError;
use crate::orchestration::types::TeamRole;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SubagentResult {
    CodeChange {
        files_touched: Vec<PathBuf>,
        summary: String,
        commit_sha: Option<String>,
    },
    ReviewFindings {
        verdict: ReviewVerdict,
        findings: Vec<Finding>,
    },
    DocsChange {
        files_touched: Vec<PathBuf>,
        summary: String,
    },
    Verdict {
        decision: String,
        rationale: String,
        new_issues: Vec<NewIssueDraft>,
    },
    Generic {
        json: serde_json::Value,
    },
}

impl SubagentResult {
    /// Tag string matching `#[serde(tag = "kind")]` on this enum.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CodeChange { .. } => "code-change",
            Self::ReviewFindings { .. } => "review-findings",
            Self::DocsChange { .. } => "docs-change",
            Self::Verdict { .. } => "verdict",
            Self::Generic { .. } => "generic",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    Approved,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub severity: FindingSeverity,
    pub note: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewIssueDraft {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub milestone: Option<u64>,
}

#[derive(Error, Debug, Clone)]
pub enum SubagentError {
    #[error("subagent timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("provider error: {0}")]
    Provider(String),
    #[error(
        "subagent returned a payload that did not match role {role:?}: expected {expected}, got {got}"
    )]
    ResultShapeMismatch {
        role: TeamRole,
        expected: String,
        got: String,
    },
    #[error("malformed parser output: {0}")]
    Malformed(String),
    #[error("subagent reported failure: {0}")]
    SubagentReported(String),
    #[error("other: {0}")]
    Other(String),
}

impl From<AgentError> for SubagentError {
    fn from(err: AgentError) -> Self {
        match err {
            AgentError::Cancelled { provider_id } => {
                SubagentError::Other(format!("{provider_id} cancelled"))
            }
            other => SubagentError::Provider(other.to_string()),
        }
    }
}

impl TeamRole {
    /// Which `SubagentResult` variants this role is allowed to produce.
    pub fn allowed_results(self) -> &'static [&'static str] {
        match self {
            Self::Implementer => &["code-change"],
            Self::Reviewer => &["review-findings", "generic"],
            Self::Docs => &["docs-change"],
            Self::Devops => &["code-change", "generic"],
            Self::Triager | Self::Researcher => &["verdict"],
            Self::Orchestrator => &["generic"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagent_result_round_trips_review_findings() {
        let r = SubagentResult::ReviewFindings {
            verdict: ReviewVerdict::Approved,
            findings: vec![Finding {
                file: Some(PathBuf::from("src/foo.rs")),
                line: Some(42),
                severity: FindingSeverity::Warn,
                note: "watch for off-by-one".into(),
            }],
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""kind":"review-findings""#));
        assert!(s.contains(r#""verdict":"approved""#));
        let _: SubagentResult = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn role_allowed_results_implementer() {
        assert_eq!(TeamRole::Implementer.allowed_results(), &["code-change"]);
    }
}
