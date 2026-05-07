//! Core orchestration types — primitives, inputs, outputs, roles.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primitive {
    Pipeline,
    FanOut,
    SinglePass,
    VerdictOnly,
}

impl Primitive {
    /// Which `TeamRole`s a given primitive requires bound for a team to be valid.
    pub fn required_roles(self) -> &'static [TeamRole] {
        match self {
            Self::Pipeline => &[TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs],
            Self::FanOut => &[TeamRole::Reviewer],
            Self::SinglePass => &[],
            Self::VerdictOnly => &[TeamRole::Reviewer],
        }
    }

    /// Canonical kebab-case label, identical to the serde tag.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pipeline => "pipeline",
            Self::FanOut => "fan-out",
            Self::SinglePass => "single-pass",
            Self::VerdictOnly => "verdict-only",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TeamInput {
    Issue {
        number: u64,
    },
    /// Set of issues, possibly spanning multiple milestones.
    /// `primary_milestone` is the wizard's reference for "same-milestone"
    /// auto-add classification (see scheduler STEP 4).
    IssueSet {
        primary_milestone: Option<u64>,
        issues: Vec<u64>,
    },
    IdeaInbox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TeamOutput {
    Pr { number: u64, branch: String },
    NewIssues { numbers: Vec<u64> },
    Comment { issue: u64, body: String },
    AdrDraft { path: PathBuf },
    Verdict { json: serde_json::Value },
    Commit { sha: String, branch: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TeamRole {
    Implementer,
    Reviewer,
    Docs,
    Devops,
    Orchestrator,
    Triager,
    Researcher,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_serde_kebab_case() {
        let p = Primitive::FanOut;
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, r#""fan-out""#);
        let back: Primitive = serde_json::from_str(&s).unwrap();
        assert_eq!(back, Primitive::FanOut);
    }

    #[test]
    fn primitive_serde_all_variants() {
        for (variant, expected) in [
            (Primitive::Pipeline, "pipeline"),
            (Primitive::FanOut, "fan-out"),
            (Primitive::SinglePass, "single-pass"),
            (Primitive::VerdictOnly, "verdict-only"),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, format!(r#""{expected}""#));
        }
    }

    #[test]
    fn pipeline_requires_three_roles() {
        let roles = Primitive::Pipeline.required_roles();
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&TeamRole::Implementer));
        assert!(roles.contains(&TeamRole::Reviewer));
        assert!(roles.contains(&TeamRole::Docs));
    }

    #[test]
    fn single_pass_has_no_required_roles() {
        assert_eq!(Primitive::SinglePass.required_roles(), &[]);
    }
}
