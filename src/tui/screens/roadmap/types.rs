//! Roadmap screen public types (#329).

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::provider::github::types::{GhIssue, GhMilestone};

/// One milestone row in the roadmap, with its issues already
/// dependency-level-sorted (by `dep_levels::dep_levels`).
#[derive(Debug, Clone)]
pub struct RoadmapEntry {
    pub milestone: GhMilestone,
    pub semver: SemVer,
    pub issues: Vec<GhIssue>,
}

/// Lightweight semver triple parsed from a milestone title like `v0.16.0`.
/// Falls back to `(0,0,0)` for non-conforming titles so they sort to the
/// end without panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };

    /// Best-effort parse: looks for `vN.N.N` or `N.N.N` anywhere in the
    /// title. Returns `None` for unparseable input.
    pub fn parse(title: &str) -> Option<Self> {
        let stripped = title.trim().trim_start_matches('v').trim_start_matches('V');
        let mut parts = stripped.split(|c: char| !c.is_ascii_digit());
        let major: u32 = parts.next()?.parse().ok()?;
        let minor: u32 = parts.next()?.parse().ok()?;
        let patch: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Parse with a fallback to `ZERO` so unparseable milestones sort first
    /// instead of being dropped.
    pub fn parse_or_zero(title: &str) -> Self {
        Self::parse(title).unwrap_or(Self::ZERO)
    }
}

/// Filters applied to the roadmap view. Empty fields mean "any".
#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub label: String,
    pub assignee: String,
    pub status: StatusFilter,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    Any,
    Open,
    Closed,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.label.is_empty()
            && self.assignee.is_empty()
            && matches!(self.status, StatusFilter::Any)
    }

    pub fn matches(&self, issue: &GhIssue) -> bool {
        if !self.label.is_empty() {
            let needle = self.label.to_lowercase();
            if !issue
                .labels
                .iter()
                .any(|l| l.to_lowercase().contains(&needle))
            {
                return false;
            }
        }
        if !self.assignee.is_empty() {
            let needle = self.assignee.to_lowercase();
            if !issue
                .assignees
                .iter()
                .any(|a| a.to_lowercase().contains(&needle))
            {
                return false;
            }
        }
        match self.status {
            StatusFilter::Any => true,
            StatusFilter::Open => issue.state.eq_ignore_ascii_case("open"),
            StatusFilter::Closed => issue.state.eq_ignore_ascii_case("closed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh_issue(state: &str, labels: &[&str], assignees: &[&str]) -> GhIssue {
        GhIssue {
            number: 1,
            title: "x".into(),
            body: String::new(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: state.into(),
            html_url: "https://example".into(),
            milestone: None,
            assignees: assignees.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn semver_parses_v_prefix() {
        let s = SemVer::parse("v0.16.0").expect("parse");
        assert_eq!(
            s,
            SemVer {
                major: 0,
                minor: 16,
                patch: 0
            }
        );
    }

    #[test]
    fn semver_parses_capital_v() {
        let s = SemVer::parse("V1.2.3").expect("parse");
        assert_eq!(
            s,
            SemVer {
                major: 1,
                minor: 2,
                patch: 3
            }
        );
    }

    #[test]
    fn semver_parses_no_prefix() {
        let s = SemVer::parse("2.0.0").expect("parse");
        assert_eq!(
            s,
            SemVer {
                major: 2,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn semver_parses_two_part_as_patch_zero() {
        let s = SemVer::parse("v0.16").expect("parse");
        assert_eq!(s.patch, 0);
        assert_eq!(s.minor, 16);
    }

    #[test]
    fn semver_unparseable_returns_none() {
        assert!(SemVer::parse("not a version").is_none());
        assert!(SemVer::parse("").is_none());
    }

    #[test]
    fn semver_ordering_major_then_minor_then_patch() {
        let a = SemVer::parse("v1.0.0").expect("p");
        let b = SemVer::parse("v2.0.0").expect("p");
        let c = SemVer::parse("v1.1.0").expect("p");
        let d = SemVer::parse("v1.0.1").expect("p");
        assert!(a < b);
        assert!(a < c);
        assert!(a < d);
        assert!(d < c);
    }

    #[test]
    fn filter_label_matches_substring_case_insensitive() {
        let f = Filters {
            label: "BUG".into(),
            ..Default::default()
        };
        assert!(f.matches(&gh_issue("open", &["bug"], &[])));
        assert!(!f.matches(&gh_issue("open", &["enhancement"], &[])));
    }

    #[test]
    fn filter_status_open_only() {
        let f = Filters {
            status: StatusFilter::Open,
            ..Default::default()
        };
        assert!(f.matches(&gh_issue("open", &[], &[])));
        assert!(!f.matches(&gh_issue("closed", &[], &[])));
    }

    #[test]
    fn filter_combined_label_and_assignee() {
        let f = Filters {
            label: "bug".into(),
            assignee: "alice".into(),
            ..Default::default()
        };
        assert!(f.matches(&gh_issue("open", &["bug"], &["alice"])));
        assert!(!f.matches(&gh_issue("open", &["bug"], &["bob"])));
        assert!(!f.matches(&gh_issue("open", &["enhancement"], &["alice"])));
    }

    #[test]
    fn filter_empty_matches_anything() {
        let f = Filters::default();
        assert!(f.is_empty());
        assert!(f.matches(&gh_issue("open", &[], &[])));
        assert!(f.matches(&gh_issue("closed", &["bug"], &["x"])));
    }
}
