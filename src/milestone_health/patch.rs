//! Patch generator: render a corrected milestone description (#500).
//!
//! The output is byte-deterministic for a given (milestone, issues, anomalies)
//! triple — required so insta snapshots are stable. We never let `HashMap`
//! iteration order leak into the output: every rendered list is sorted by
//! issue number ascending.

use std::collections::{BTreeMap, HashSet};

use crate::milestone_health::graph::{compute_levels, parse_graph};
use crate::milestone_health::types::GraphAnomaly;
use crate::provider::github::types::{GhIssue, GhMilestone};

const GRAPH_HEADING: &str = "## Dependency Graph (Implementation Order)";

/// Generate a corrected milestone description.
///
/// Rules:
/// - Preserves any text that appears before the original `## Dependency
///   Graph` heading (the milestone "summary line" + paragraphs).
/// - Re-emits the dependency graph using `compute_levels()` ordering.
/// - Preserves `✅` markers from the original graph for any issue number
///   that bore one.
/// - Emits issue lines as `• #NNN <title>` if a title is known, else `• #NNN`.
/// - Annotates cycle members with a `(cycle)` warning suffix — they cannot
///   be placed at a deterministic level until the cycle is broken.
pub fn generate_patch(
    milestone: &GhMilestone,
    issues: &[GhIssue],
    anomalies: &[GraphAnomaly],
) -> String {
    let preamble = preamble_text(&milestone.description);

    let titles: BTreeMap<u64, String> =
        issues.iter().map(|i| (i.number, i.title.clone())).collect();
    let original = parse_graph(&milestone.description);
    let preserved_completed: HashSet<u64> = original.completed_issues().into_iter().collect();

    let milestone_set: HashSet<u64> = issues.iter().map(|i| i.number).collect();
    let levels_map = compute_levels(issues, &milestone_set);

    let mut by_level: BTreeMap<usize, Vec<u64>> = BTreeMap::new();
    let mut cycle_members_sorted: Vec<u64> = Vec::new();
    for (issue_num, lvl) in &levels_map {
        match lvl {
            Some(n) => by_level.entry(*n).or_default().push(*issue_num),
            None => cycle_members_sorted.push(*issue_num),
        }
    }
    for v in by_level.values_mut() {
        v.sort_unstable();
    }
    cycle_members_sorted.sort_unstable();

    let mut out = String::new();
    if !preamble.is_empty() {
        out.push_str(preamble.trim_end());
        out.push_str("\n\n");
    }

    out.push_str(GRAPH_HEADING);
    out.push_str("\n\n");

    if by_level.is_empty() && cycle_members_sorted.is_empty() {
        out.push_str("(no issues)\n");
    } else {
        for (lvl, members) in &by_level {
            let depends = if *lvl == 0 {
                "no dependencies".to_string()
            } else {
                format!("depends on Level {}", lvl - 1)
            };
            out.push_str(&format!("Level {} — {}:\n", lvl, depends));
            for n in members {
                emit_issue_line(&mut out, *n, &titles, preserved_completed.contains(n));
            }
            out.push('\n');
        }

        if !cycle_members_sorted.is_empty() {
            out.push_str("Cycle members (resolve before assigning levels):\n");
            for n in &cycle_members_sorted {
                emit_issue_line(&mut out, *n, &titles, preserved_completed.contains(n));
            }
            out.push('\n');
        }

        out.push_str(&sequence_line(&by_level));
        out.push('\n');
    }

    if anomalies
        .iter()
        .any(|a| matches!(a, GraphAnomaly::CircularDependency { .. }))
    {
        out.push_str(
            "\n> Note: cycles were detected and listed above; resolve them before merging.\n",
        );
    }

    out
}

fn emit_issue_line(out: &mut String, number: u64, titles: &BTreeMap<u64, String>, completed: bool) {
    let title = titles
        .get(&number)
        .map(|s| s.as_str())
        .unwrap_or("placeholder");
    let prefix = if completed { "• ✅ " } else { "• " };
    out.push_str(&format!("{}#{} {}\n", prefix, number, title));
}

fn sequence_line(by_level: &BTreeMap<usize, Vec<u64>>) -> String {
    if by_level.is_empty() {
        return String::from("Sequence: (empty)");
    }
    let parts: Vec<String> = by_level
        .values()
        .map(|members| {
            members
                .iter()
                .map(|n| format!("#{}", n))
                .collect::<Vec<_>>()
                .join(" ∥ ")
        })
        .collect();
    format!("Sequence: {}", parts.join(" → "))
}

fn preamble_text(description: &str) -> String {
    let mut out = String::new();
    for line in description.split_inclusive('\n') {
        let trimmed = line.trim_end();
        if trimmed.starts_with("## Dependency Graph") {
            break;
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::types::{GhIssue, GhMilestone};

    fn ms(description: &str) -> GhMilestone {
        GhMilestone {
            number: 1,
            title: "v1.0".to_string(),
            description: description.to_string(),
            state: "open".to_string(),
            open_issues: 0,
            closed_issues: 0,
        }
    }

    fn issue(number: u64, title: &str, blockers: &[u64]) -> GhIssue {
        let mut body = format!("## Overview\n\n{}\n\n## Blocked By\n\n", title);
        if blockers.is_empty() {
            body.push_str("- None\n");
        } else {
            for b in blockers {
                body.push_str(&format!("- #{} placeholder\n", b));
            }
        }
        GhIssue {
            number,
            title: title.to_string(),
            body,
            labels: vec!["type:feature".to_string()],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: Some(1),
            assignees: vec![],
        }
    }

    // C-1
    #[test]
    fn patch_wrong_level_fix() {
        let m = ms(
            "Header summary.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0:\n• #1 a\n• #2 b\n• #3 c\n\nSequence: #1 ∥ #2 ∥ #3\n",
        );
        let issues = vec![issue(1, "a", &[]), issue(2, "b", &[]), issue(3, "c", &[2])];
        let anomalies = vec![GraphAnomaly::WrongLevelAssignment {
            issue: 3,
            expected: 1,
            found: 0,
        }];
        let out = generate_patch(&m, &issues, &anomalies);
        insta::assert_snapshot!("milestone_health__patch_wrong_level_fix", out);
    }

    // C-2
    #[test]
    fn patch_cycle_break_note() {
        let m = ms(
            "Header.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0:\n• #4 a\n• #5 b\n",
        );
        let issues = vec![issue(4, "a", &[5]), issue(5, "b", &[4])];
        let anomalies = vec![GraphAnomaly::CircularDependency { cycle: vec![4, 5] }];
        let out = generate_patch(&m, &issues, &anomalies);
        insta::assert_snapshot!("milestone_health__patch_cycle_break", out);
    }

    // C-3
    #[test]
    fn patch_add_missing_issue() {
        let m = ms("Header.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0:\n• #1 a\n");
        let issues = vec![issue(1, "a", &[]), issue(7, "g", &[])];
        let anomalies = vec![GraphAnomaly::IssueMissingFromGraph { issue: 7 }];
        let out = generate_patch(&m, &issues, &anomalies);
        insta::assert_snapshot!("milestone_health__patch_add_missing_issue", out);
    }

    // C-4
    #[test]
    fn patch_preserves_completed_markers() {
        let m = ms(
            "Header.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0:\n• ✅ #1 done\n\nLevel 1:\n• #2 wip\n",
        );
        let issues = vec![issue(1, "done", &[]), issue(2, "wip", &[1])];
        let out = generate_patch(&m, &issues, &[]);
        insta::assert_snapshot!("milestone_health__patch_preserves_completed", out);
    }

    // C-5
    #[test]
    fn patch_sequence_line_sequential() {
        let m = ms("Header.");
        let issues = vec![issue(1, "a", &[]), issue(2, "b", &[1]), issue(3, "c", &[2])];
        let out = generate_patch(&m, &issues, &[]);
        insta::assert_snapshot!("milestone_health__patch_sequence_sequential", out);
    }

    // C-6
    #[test]
    fn patch_sequence_line_parallel() {
        let m = ms("Header.");
        let issues = vec![
            issue(1, "a", &[]),
            issue(2, "b", &[1]),
            issue(3, "c", &[1]),
            issue(4, "d", &[2, 3]),
        ];
        let out = generate_patch(&m, &issues, &[]);
        insta::assert_snapshot!("milestone_health__patch_sequence_parallel", out);
    }

    // C-7
    #[test]
    fn patch_empty_anomalies_canonical_reemit() {
        let m = ms("Header.");
        let issues = vec![issue(1, "a", &[]), issue(2, "b", &[1]), issue(3, "c", &[2])];
        let out = generate_patch(&m, &issues, &[]);
        insta::assert_snapshot!("milestone_health__patch_no_anomalies_canonical", out);
    }

    // C-8
    #[test]
    fn patch_output_is_deterministic_across_calls() {
        let m = ms("Header.");
        let issues = vec![
            issue(11, "a", &[]),
            issue(22, "b", &[]),
            issue(33, "c", &[]),
        ];
        let out1 = generate_patch(&m, &issues, &[]);
        let out2 = generate_patch(&m, &issues, &[]);
        assert_eq!(out1, out2);
    }
}
