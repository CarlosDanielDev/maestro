//! Source selection for the PRD generator — local file, GitHub issue,
//! Azure DevOps, or a merge of local + remote.
//!
//! The source drives whether the Consolidate phase generates from scratch
//! or *enriches* an existing PRD:
//!
//! - `Local` — read/write `docs/PRD.md` (legacy behavior)
//! - `GitHub` — fetch a pinned / `prd`-labeled issue; write back as a comment
//! - `Azure` — fetch from an Azure DevOps wiki page or work item
//! - `Both` — merge the local file + the selected remote provider

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Where the PRD lives for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum PrdSource {
    /// Read/write the local `docs/PRD.md` (default and legacy behavior).
    #[default]
    Local,
    /// Fetch the PRD from a GitHub pinned issue or issue labeled `prd`.
    Github,
    /// Fetch the PRD from Azure DevOps (wiki page or work item).
    Azure,
    /// Merge the local file with the GitHub issue content.
    Both,
}

#[allow(
    dead_code,
    reason = "PrdSource helpers are called from the TUI wizard cycling handlers and from tests; suppressing dead-code until the adapt wizard wires them in full."
)]
impl PrdSource {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Local => "Local file",
            Self::Github => "GitHub issue",
            Self::Azure => "Azure DevOps",
            Self::Both => "Local + GitHub",
        }
    }

    /// Cycle to the next source (for TUI j/k navigation).
    pub const fn next(&self) -> Self {
        match self {
            Self::Local => Self::Github,
            Self::Github => Self::Azure,
            Self::Azure => Self::Both,
            Self::Both => Self::Local,
        }
    }

    /// Cycle to the previous source.
    pub const fn previous(&self) -> Self {
        match self {
            Self::Local => Self::Both,
            Self::Github => Self::Local,
            Self::Azure => Self::Github,
            Self::Both => Self::Azure,
        }
    }

    /// All sources in cycle order — useful for tests and settings UIs.
    pub const fn all() -> [Self; 4] {
        [Self::Local, Self::Github, Self::Azure, Self::Both]
    }

    /// True if this source implies reading from a remote provider.
    pub const fn uses_remote(&self) -> bool {
        !matches!(self, Self::Local)
    }

    /// True if this source implies reading/writing the local file.
    pub const fn uses_local(&self) -> bool {
        matches!(self, Self::Local | Self::Both)
    }
}

/// Fetched PRD content alongside where it came from, so we can write back
/// to the same destination after the enrichment pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedPrd {
    pub content: String,
    pub origin: PrdOrigin,
}

/// Concrete resource the PRD was read from. Absence of an existing PRD is
/// represented by `Option<FetchedPrd>::None`, so this enum only describes
/// found-it origins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrdOrigin {
    Local { path: std::path::PathBuf },
    GithubIssue { number: u64 },
    AzureWiki { project: String, page: String },
}

impl PrdOrigin {
    pub fn describe(&self) -> String {
        match self {
            Self::Local { path } => format!("local file {}", path.display()),
            Self::GithubIssue { number } => format!("GitHub issue #{}", number),
            Self::AzureWiki { project, page } => format!("Azure wiki {}/{}", project, page),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_local() {
        assert_eq!(PrdSource::default(), PrdSource::Local);
    }

    #[test]
    fn serde_round_trip_preserves_variant() {
        for source in PrdSource::all() {
            let json = serde_json::to_string(&source).unwrap();
            let rt: PrdSource = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, source, "round-trip failed for {:?}", source);
        }
    }

    #[test]
    fn next_and_previous_are_inverse() {
        for source in PrdSource::all() {
            assert_eq!(
                source.next().previous(),
                source,
                "next().previous() should be identity for {:?}",
                source
            );
        }
    }

    #[test]
    fn next_cycles_through_all_variants() {
        let mut s = PrdSource::Local;
        let cycled: Vec<_> = (0..4)
            .map(|_| {
                let r = s;
                s = s.next();
                r
            })
            .collect();
        assert_eq!(cycled.len(), 4);
        for variant in PrdSource::all() {
            assert!(cycled.contains(&variant));
        }
        assert_eq!(s, PrdSource::Local, "should cycle back to start");
    }

    #[test]
    fn labels_are_non_empty_and_distinct() {
        let labels: Vec<&str> = PrdSource::all().iter().map(|s| s.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(unique.len(), labels.len(), "labels must be distinct");
        assert!(labels.iter().all(|l| !l.is_empty()));
    }

    #[test]
    fn uses_remote_true_for_non_local() {
        assert!(!PrdSource::Local.uses_remote());
        assert!(PrdSource::Github.uses_remote());
        assert!(PrdSource::Azure.uses_remote());
        assert!(PrdSource::Both.uses_remote());
    }

    #[test]
    fn uses_local_includes_both() {
        assert!(PrdSource::Local.uses_local());
        assert!(PrdSource::Both.uses_local());
        assert!(!PrdSource::Github.uses_local());
        assert!(!PrdSource::Azure.uses_local());
    }

    #[test]
    fn prd_origin_describe_non_empty() {
        let origins = [
            PrdOrigin::Local {
                path: std::path::PathBuf::from("docs/PRD.md"),
            },
            PrdOrigin::GithubIssue { number: 42 },
            PrdOrigin::AzureWiki {
                project: "p".into(),
                page: "PRD".into(),
            },
        ];
        for o in &origins {
            assert!(!o.describe().is_empty());
        }
    }

    #[test]
    fn snake_case_serde_representation() {
        let json = serde_json::to_string(&PrdSource::Github).unwrap();
        assert_eq!(json, r#""github""#);
    }
}
