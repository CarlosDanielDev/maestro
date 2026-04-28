//! Stringification helpers for the milestone health report (#500).

use crate::milestone_health::types::{GraphAnomaly, MissingField};

pub fn missing_fields(missing: &[MissingField]) -> String {
    use MissingField::*;
    missing
        .iter()
        .map(|m| match m {
            Section(s) => (*s).to_string(),
            WeakAcceptanceCriteria => "weak Acceptance Criteria".into(),
            WeakBlockedBy => "weak Blocked By".into(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn anomaly(a: &GraphAnomaly) -> String {
    use GraphAnomaly::*;
    match a {
        MissingDependencyGraphSection => "missing `## Dependency Graph` section".into(),
        MissingSequenceLine => "missing `Sequence:` line".into(),
        CircularDependency { cycle } => format!(
            "circular dependency: {}",
            cycle
                .iter()
                .map(|n| format!("#{}", n))
                .collect::<Vec<_>>()
                .join(" → ")
        ),
        CrossMilestoneBlockedBy { issue, blocker } => {
            format!("WARN  #{issue} is blocked by #{blocker} (outside milestone)")
        }
        WrongLevelAssignment {
            issue,
            expected,
            found,
        } => format!("#{issue} at Level {found} but should be at Level {expected}"),
        IssueMissingFromGraph { issue } => format!("#{issue} missing from graph"),
        GraphReferencesUnknownIssue { issue } => format!("graph references unknown issue #{issue}"),
    }
}
