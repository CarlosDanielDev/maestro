//! Tests for the dependency-graph parser, level computer, cycle
//! detector, and analyzer (#500).

use super::*;
use crate::provider::github::types::GhIssue;

fn issue_blocked_by(number: u64, blockers: &[u64], milestone: Option<u64>) -> GhIssue {
    let body = if blockers.is_empty() {
        "## Blocked By\n\n- None\n".to_string()
    } else {
        let mut s = String::from("## Blocked By\n\n");
        for b in blockers {
            s.push_str(&format!("- #{} placeholder\n", b));
        }
        s
    };
    GhIssue {
        number,
        title: format!("Issue #{}", number),
        body,
        labels: vec!["type:feature".to_string()],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone,
        assignees: vec![],
    }
}

fn well_formed_graph_description(levels: &[&[u64]]) -> String {
    let mut s = String::new();
    s.push_str("Summary line.\n\n## Dependency Graph (Implementation Order)\n\n");
    for (i, lvl) in levels.iter().enumerate() {
        let depends = if i == 0 {
            "no dependencies".to_string()
        } else {
            format!("depends on Level {}", i - 1)
        };
        s.push_str(&format!("Level {} — {}:\n", i, depends));
        for n in *lvl {
            s.push_str(&format!("• #{} placeholder\n", n));
        }
        s.push('\n');
    }
    let seq: Vec<String> = levels
        .iter()
        .map(|lvl| {
            lvl.iter()
                .map(|n| format!("#{}", n))
                .collect::<Vec<_>>()
                .join(" ∥ ")
        })
        .collect();
    s.push_str(&format!("Sequence: {}\n", seq.join(" → ")));
    s
}

fn ms_set(numbers: &[u64]) -> HashSet<u64> {
    numbers.iter().copied().collect()
}

#[test]
fn parse_graph_well_formed_returns_levels_and_sequence() {
    let desc = well_formed_graph_description(&[&[10], &[20, 21], &[30]]);
    let parsed = parse_graph(&desc);
    assert_eq!(parsed.levels.len(), 3);
    assert_eq!(parsed.levels[0].issues, vec![10]);
    assert_eq!(parsed.levels[1].issues, vec![20, 21]);
    assert!(parsed.sequence_line.is_some());
}

#[test]
fn parse_graph_empty_description_returns_empty_levels() {
    let parsed = parse_graph("One-liner summary.");
    assert!(parsed.levels.is_empty());
    assert!(parsed.sequence_line.is_none());
}

#[test]
fn parse_graph_missing_section_header_returns_empty() {
    let parsed = parse_graph("## Overview\n\nhi\n");
    assert!(parsed.levels.is_empty());
}

#[test]
fn parse_graph_preserves_completed_markers() {
    let desc = "## Dependency Graph (Implementation Order)\n\nLevel 0:\n• ✅ #5 done\n\nLevel 1:\n• #6 wip\n";
    let parsed = parse_graph(desc);
    assert_eq!(parsed.levels.len(), 2);
    assert_eq!(parsed.levels[0].completed, vec![5]);
    assert_eq!(parsed.levels[1].issues, vec![6]);
    assert!(parsed.levels[1].completed.is_empty());
}

#[test]
fn parse_graph_sequence_line_extracted() {
    let parsed = parse_graph(
        "## Dependency Graph\n\nLevel 0:\n• #10 a\n\nSequence: #10 → #20 ∥ #21 → #30\n",
    );
    assert_eq!(
        parsed.sequence_line.as_deref(),
        Some("#10 → #20 ∥ #21 → #30")
    );
}

#[test]
fn compute_levels_simple_chain() {
    let issues = vec![
        issue_blocked_by(1, &[], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
        issue_blocked_by(3, &[2], Some(1)),
    ];
    let lvls = compute_levels(&issues, &ms_set(&[1, 2, 3]));
    assert_eq!(lvls[&1], Some(0));
    assert_eq!(lvls[&2], Some(1));
    assert_eq!(lvls[&3], Some(2));
}

#[test]
fn compute_levels_parallel_siblings() {
    let issues = vec![
        issue_blocked_by(1, &[], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
        issue_blocked_by(3, &[1], Some(1)),
    ];
    let lvls = compute_levels(&issues, &ms_set(&[1, 2, 3]));
    assert_eq!(lvls[&1], Some(0));
    assert_eq!(lvls[&2], Some(1));
    assert_eq!(lvls[&3], Some(1));
}

#[test]
fn compute_levels_isolated_node_is_level_zero() {
    let issues = vec![issue_blocked_by(7, &[], Some(1))];
    let lvls = compute_levels(&issues, &ms_set(&[7]));
    assert_eq!(lvls[&7], Some(0));
}

#[test]
fn compute_levels_external_blocker_does_not_raise_level() {
    let issues = vec![issue_blocked_by(10, &[99], Some(1))];
    let lvls = compute_levels(&issues, &ms_set(&[10]));
    assert_eq!(lvls[&10], Some(0));
}

#[test]
fn compute_levels_cycle_yields_none() {
    let issues = vec![
        issue_blocked_by(5, &[6], Some(1)),
        issue_blocked_by(6, &[5], Some(1)),
    ];
    let lvls = compute_levels(&issues, &ms_set(&[5, 6]));
    assert_eq!(lvls[&5], None);
    assert_eq!(lvls[&6], None);
}

#[test]
fn detect_cycles_empty_graph_no_cycles() {
    assert!(detect_cycles(&[]).is_empty());
}

#[test]
fn detect_cycles_acyclic_graph_no_cycles() {
    let issues = vec![
        issue_blocked_by(1, &[], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
    ];
    assert!(detect_cycles(&issues).is_empty());
}

#[test]
fn detect_cycles_simple_two_node_cycle() {
    let issues = vec![
        issue_blocked_by(1, &[2], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
    ];
    let cycles = detect_cycles(&issues);
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0], vec![1, 2]);
}

#[test]
fn detect_cycles_three_node_cycle() {
    let issues = vec![
        issue_blocked_by(3, &[5], Some(1)),
        issue_blocked_by(5, &[7], Some(1)),
        issue_blocked_by(7, &[3], Some(1)),
    ];
    let cycles = detect_cycles(&issues);
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0], vec![3, 5, 7]);
}

#[test]
fn detect_cycles_two_disjoint_cycles_deterministic_order() {
    let issues = vec![
        issue_blocked_by(10, &[11], Some(1)),
        issue_blocked_by(11, &[10], Some(1)),
        issue_blocked_by(1, &[2], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
    ];
    let cycles = detect_cycles(&issues);
    assert_eq!(cycles.len(), 2);
    assert_eq!(cycles[0], vec![1, 2]);
    assert_eq!(cycles[1], vec![10, 11]);
}

#[test]
fn analyze_well_formed_graph_zero_anomalies() {
    let issues = vec![
        issue_blocked_by(1, &[], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
        issue_blocked_by(3, &[2], Some(1)),
    ];
    let desc = well_formed_graph_description(&[&[1], &[2], &[3]]);
    let anomalies = analyze(&desc, &issues);
    assert!(anomalies.is_empty(), "got: {:?}", anomalies);
}

#[test]
fn analyze_circular_dependency_yields_cycle_anomaly() {
    let issues = vec![
        issue_blocked_by(1, &[2], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
    ];
    let desc = well_formed_graph_description(&[&[1, 2]]);
    let anomalies = analyze(&desc, &issues);
    assert!(
        anomalies.iter().any(
            |a| matches!(a, GraphAnomaly::CircularDependency { cycle } if cycle == &vec![1, 2])
        )
    );
}

#[test]
fn analyze_cross_milestone_blocked_by_yields_anomaly() {
    let issues = vec![issue_blocked_by(5, &[99], Some(1))];
    let desc = well_formed_graph_description(&[&[5]]);
    let anomalies = analyze(&desc, &issues);
    assert!(anomalies.iter().any(|a| matches!(
        a,
        GraphAnomaly::CrossMilestoneBlockedBy {
            issue: 5,
            blocker: 99
        }
    )));
}

#[test]
fn analyze_missing_dependency_graph_section() {
    let issues = vec![issue_blocked_by(1, &[], Some(1))];
    let anomalies = analyze("One-line summary only.", &issues);
    assert!(
        anomalies
            .iter()
            .any(|a| matches!(a, GraphAnomaly::MissingDependencyGraphSection))
    );
}

#[test]
fn analyze_wrong_level_assignment() {
    let issues = vec![
        issue_blocked_by(1, &[], Some(1)),
        issue_blocked_by(2, &[1], Some(1)),
    ];
    // Place #2 wrongly at Level 0.
    let desc = well_formed_graph_description(&[&[1, 2]]);
    let anomalies = analyze(&desc, &issues);
    assert!(anomalies.iter().any(|a| matches!(
        a,
        GraphAnomaly::WrongLevelAssignment {
            issue: 2,
            expected: 1,
            found: 0,
        }
    )));
}

#[test]
fn analyze_issue_missing_from_graph() {
    let issues = vec![
        issue_blocked_by(1, &[], Some(1)),
        issue_blocked_by(2, &[], Some(1)),
    ];
    let desc = well_formed_graph_description(&[&[1]]); // omits #2
    let anomalies = analyze(&desc, &issues);
    assert!(
        anomalies
            .iter()
            .any(|a| matches!(a, GraphAnomaly::IssueMissingFromGraph { issue: 2 }))
    );
}

#[test]
fn analyze_graph_references_unknown_issue() {
    let issues = vec![issue_blocked_by(1, &[], Some(1))];
    let desc = well_formed_graph_description(&[&[1, 99]]);
    let anomalies = analyze(&desc, &issues);
    assert!(
        anomalies
            .iter()
            .any(|a| matches!(a, GraphAnomaly::GraphReferencesUnknownIssue { issue: 99 }))
    );
}
