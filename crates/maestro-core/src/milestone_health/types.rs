//! Seam types for the milestone-health analysis layer (#500).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueType {
    Feature,
    Bug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingField {
    Section(&'static str),
    WeakAcceptanceCriteria,
    WeakBlockedBy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DorResult {
    pub issue_number: u64,
    pub issue_type: IssueType,
    pub missing: Vec<MissingField>,
}

impl DorResult {
    pub fn passed(&self) -> bool {
        self.missing.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedBySection {
    None,
    Issues(Vec<u64>),
    Weak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphAnomaly {
    MissingDependencyGraphSection,
    MissingSequenceLine,
    CircularDependency {
        cycle: Vec<u64>,
    },
    CrossMilestoneBlockedBy {
        issue: u64,
        blocker: u64,
    },
    WrongLevelAssignment {
        issue: u64,
        expected: usize,
        found: usize,
    },
    IssueMissingFromGraph {
        issue: u64,
    },
    GraphReferencesUnknownIssue {
        issue: u64,
    },
}

/// One level row parsed out of the milestone's `## Dependency Graph` block.
/// `completed` lists the issue numbers in this level that bore a `✅` marker.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphLevel {
    pub level: usize,
    pub issues: Vec<u64>,
    pub completed: Vec<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedGraph {
    pub levels: Vec<GraphLevel>,
    pub sequence_line: Option<String>,
}

impl ParsedGraph {
    /// Map each issue → the level it appears in (if any).
    pub fn issue_to_level(&self) -> HashMap<u64, usize> {
        let mut map = HashMap::new();
        for lvl in &self.levels {
            for issue in &lvl.issues {
                map.insert(*issue, lvl.level);
            }
        }
        map
    }

    /// Flat set of all issue numbers referenced by the graph.
    pub fn all_issues(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self.levels.iter().flat_map(|l| l.issues.clone()).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Issue numbers that bore a ✅ marker in the original graph.
    pub fn completed_issues(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self
            .levels
            .iter()
            .flat_map(|l| l.completed.clone())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}
