//! Dependency-graph parser, level computer, cycle detector, analyzer (#500).

use std::collections::{HashMap, HashSet};

use crate::milestone_health::dor::parse_blocked_by_section;
use crate::milestone_health::types::{BlockedBySection, GraphAnomaly, GraphLevel, ParsedGraph};
use crate::provider::github::types::GhIssue;

/// Gather all blocker issue numbers for an issue: union of labels,
/// inline `blocked-by:` references, and the structured `## Blocked By`
/// section. Deduplicated and sorted.
pub fn gather_blockers(issue: &GhIssue) -> Vec<u64> {
    let mut all = issue.all_blockers();
    if let Some(BlockedBySection::Issues(nums)) = parse_blocked_by_section(&issue.body) {
        all.extend(nums);
    }
    all.sort_unstable();
    all.dedup();
    all
}

const GRAPH_HEADING: &str = "## Dependency Graph";

/// Parse the milestone description's `## Dependency Graph` block plus the
/// `Sequence:` line. Tolerates the `(Implementation Order)` suffix and
/// the optional `(COMPLETED ✅)` level suffix.
pub fn parse_graph(description: &str) -> ParsedGraph {
    let mut levels: Vec<GraphLevel> = Vec::new();
    let mut sequence_line: Option<String> = None;
    let mut in_block = false;
    let mut current_level: Option<GraphLevel> = None;

    for raw in description.lines() {
        let trimmed = raw.trim_end();

        if trimmed.starts_with(GRAPH_HEADING) {
            in_block = true;
            continue;
        }

        if !in_block {
            continue;
        }

        // A new top-level `## ...` heading ends the graph block.
        if trimmed.starts_with("## ") && !trimmed.starts_with(GRAPH_HEADING) {
            break;
        }

        if let Some(rest) = trimmed.trim_start().strip_prefix("Sequence:") {
            sequence_line = Some(rest.trim().to_string());
            continue;
        }

        if let Some(level) = parse_level_header(trimmed) {
            if let Some(prev) = current_level.take() {
                levels.push(prev);
            }
            current_level = Some(GraphLevel {
                level,
                issues: Vec::new(),
                completed: Vec::new(),
            });
            continue;
        }

        if let Some(lvl) = current_level.as_mut()
            && let Some((issue, completed)) = parse_issue_bullet(trimmed)
        {
            lvl.issues.push(issue);
            if completed {
                lvl.completed.push(issue);
            }
        }
    }

    if let Some(prev) = current_level.take() {
        levels.push(prev);
    }

    ParsedGraph {
        levels,
        sequence_line,
    }
}

fn parse_level_header(line: &str) -> Option<usize> {
    let rest = line.trim_start().strip_prefix("Level ")?;
    let n_end = rest
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    rest[..n_end].parse::<usize>().ok()
}

fn parse_issue_bullet(line: &str) -> Option<(u64, bool)> {
    let rest = line
        .trim_start()
        .strip_prefix("• ")
        .or_else(|| line.trim_start().strip_prefix("- "))?;
    let (rest, completed) = match rest.trim_start().strip_prefix("✅") {
        Some(stripped) => (stripped.trim_start(), true),
        None => (rest.trim_start(), false),
    };
    let digits: String = rest
        .strip_prefix('#')?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u64>().ok().map(|n| (n, completed))
}

/// Build a map from each issue number to its blockers (gathered from labels,
/// inline body refs, and the structured `## Blocked By` section). Computing
/// this once and reusing it across `detect_cycles` and `compute_levels`
/// avoids re-parsing each issue body O(N) times per `analyze` call.
fn build_blockers_map(issues: &[GhIssue]) -> HashMap<u64, Vec<u64>> {
    issues
        .iter()
        .map(|i| (i.number, gather_blockers(i)))
        .collect()
}

/// Compute the dependency level for each issue. Issues whose blockers are
/// outside the milestone get level 0 (external blockers are surfaced as
/// `CrossMilestoneBlockedBy` anomalies, not level shifts). Cycle members
/// get `None`.
pub fn compute_levels(
    issues: &[GhIssue],
    milestone_set: &HashSet<u64>,
) -> HashMap<u64, Option<usize>> {
    let blockers_map = build_blockers_map(issues);
    let cycles = detect_cycles_with_blockers(issues, &blockers_map);
    compute_levels_with(issues, milestone_set, &blockers_map, &cycles)
}

fn compute_levels_with(
    issues: &[GhIssue],
    milestone_set: &HashSet<u64>,
    blockers_map: &HashMap<u64, Vec<u64>>,
    cycles: &[Vec<u64>],
) -> HashMap<u64, Option<usize>> {
    let internal: HashMap<u64, Vec<u64>> = blockers_map
        .iter()
        .map(|(n, bs)| {
            (
                *n,
                bs.iter()
                    .copied()
                    .filter(|b| milestone_set.contains(b))
                    .collect(),
            )
        })
        .collect();
    let cycle_members: HashSet<u64> = cycles.iter().flatten().copied().collect();
    let mut levels: HashMap<u64, Option<usize>> = HashMap::new();

    let mut progress = true;
    while progress {
        progress = false;
        for issue in issues {
            if levels.contains_key(&issue.number) {
                continue;
            }
            if cycle_members.contains(&issue.number) {
                levels.insert(issue.number, None);
                progress = true;
                continue;
            }
            let bs = match internal.get(&issue.number) {
                Some(v) if !v.is_empty() => v,
                _ => {
                    levels.insert(issue.number, Some(0));
                    progress = true;
                    continue;
                }
            };
            let resolved: Option<Vec<usize>> = bs
                .iter()
                .map(|b| levels.get(b).and_then(|opt| *opt))
                .collect();
            if let Some(deps) = resolved {
                let lvl = deps.into_iter().max().map(|m| m + 1).unwrap_or(0);
                levels.insert(issue.number, Some(lvl));
                progress = true;
            }
        }
    }
    for issue in issues {
        levels.entry(issue.number).or_insert(None);
    }
    levels
}

/// Tarjan-style SCC detection. Returns one `Vec<u64>` per non-trivial
/// strongly connected component (size ≥ 2, or self-loop). Determinism:
/// SCCs are sorted by minimum issue number; within an SCC, members are
/// sorted ascending.
#[allow(dead_code)] // Reason: public ergonomic wrapper; production uses detect_cycles_with_blockers, tests call this directly
pub fn detect_cycles(issues: &[GhIssue]) -> Vec<Vec<u64>> {
    let blockers_map = build_blockers_map(issues);
    detect_cycles_with_blockers(issues, &blockers_map)
}

fn detect_cycles_with_blockers(
    issues: &[GhIssue],
    blockers_map: &HashMap<u64, Vec<u64>>,
) -> Vec<Vec<u64>> {
    let nodes: Vec<u64> = issues.iter().map(|i| i.number).collect();
    let node_set: HashSet<u64> = nodes.iter().copied().collect();
    let adj: HashMap<u64, Vec<u64>> = blockers_map
        .iter()
        .map(|(n, bs)| {
            (
                *n,
                bs.iter()
                    .copied()
                    .filter(|b| node_set.contains(b))
                    .collect(),
            )
        })
        .collect();

    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut lowlink: HashMap<u64, usize> = HashMap::new();
    let mut on_stack: HashSet<u64> = HashSet::new();
    let mut stack: Vec<u64> = Vec::new();
    let mut counter: usize = 0;
    let mut sccs: Vec<Vec<u64>> = Vec::new();

    // Sort nodes for deterministic traversal.
    let mut traversal = nodes;
    traversal.sort_unstable();

    for &v in &traversal {
        if !index.contains_key(&v) {
            strongconnect(
                v,
                &adj,
                &mut index,
                &mut lowlink,
                &mut on_stack,
                &mut stack,
                &mut counter,
                &mut sccs,
            );
        }
    }

    // Filter to non-trivial cycles + sort.
    let mut filtered: Vec<Vec<u64>> = sccs
        .into_iter()
        .filter(|scc| {
            if scc.len() >= 2 {
                return true;
            }
            // Self-loop check: a single-node SCC where the node lists itself.
            if let Some(&n) = scc.first()
                && let Some(neighbors) = adj.get(&n)
            {
                return neighbors.contains(&n);
            }
            false
        })
        .map(|mut scc| {
            scc.sort_unstable();
            scc
        })
        .collect();

    filtered.sort_by_key(|scc| scc.first().copied().unwrap_or(u64::MAX));
    filtered
}

#[allow(clippy::too_many_arguments)]
fn strongconnect(
    v: u64,
    adj: &HashMap<u64, Vec<u64>>,
    index: &mut HashMap<u64, usize>,
    lowlink: &mut HashMap<u64, usize>,
    on_stack: &mut HashSet<u64>,
    stack: &mut Vec<u64>,
    counter: &mut usize,
    sccs: &mut Vec<Vec<u64>>,
) {
    index.insert(v, *counter);
    lowlink.insert(v, *counter);
    *counter += 1;
    stack.push(v);
    on_stack.insert(v);

    let mut neighbors = adj.get(&v).cloned().unwrap_or_default();
    neighbors.sort_unstable();
    for w in neighbors {
        if !index.contains_key(&w) {
            strongconnect(w, adj, index, lowlink, on_stack, stack, counter, sccs);
            let lw = *lowlink.get(&w).unwrap_or(&usize::MAX);
            let lv = *lowlink.get(&v).unwrap_or(&usize::MAX);
            lowlink.insert(v, lv.min(lw));
        } else if on_stack.contains(&w) {
            let iw = *index.get(&w).unwrap_or(&usize::MAX);
            let lv = *lowlink.get(&v).unwrap_or(&usize::MAX);
            lowlink.insert(v, lv.min(iw));
        }
    }

    if lowlink.get(&v) == index.get(&v) {
        let mut scc = Vec::new();
        while let Some(w) = stack.pop() {
            on_stack.remove(&w);
            scc.push(w);
            if w == v {
                break;
            }
        }
        sccs.push(scc);
    }
}

/// Top-level analyzer for a milestone's open issues + the milestone
/// description. Returns the union of all `GraphAnomaly` variants found.
///
/// Builds the blockers map, cycle set, and computed levels exactly once
/// internally — earlier revisions ran detect_cycles three times and
/// compute_levels twice on the same inputs.
pub fn analyze(description: &str, issues: &[GhIssue]) -> Vec<GraphAnomaly> {
    let mut out: Vec<GraphAnomaly> = Vec::new();
    let parsed = parse_graph(description);
    let milestone_set: HashSet<u64> = issues.iter().map(|i| i.number).collect();
    let blockers_map = build_blockers_map(issues);
    let cycles = detect_cycles_with_blockers(issues, &blockers_map);
    let cycle_members: HashSet<u64> = cycles.iter().flatten().copied().collect();

    if parsed.levels.is_empty() {
        out.push(GraphAnomaly::MissingDependencyGraphSection);
    } else if parsed.sequence_line.is_none() {
        out.push(GraphAnomaly::MissingSequenceLine);
    }

    for cycle in &cycles {
        out.push(GraphAnomaly::CircularDependency {
            cycle: cycle.clone(),
        });
    }

    let mut cross_pairs: Vec<(u64, u64)> = Vec::new();
    for (issue, blockers) in &blockers_map {
        for blocker in blockers {
            if !milestone_set.contains(blocker) {
                cross_pairs.push((*issue, *blocker));
            }
        }
    }
    cross_pairs.sort_unstable();
    for (issue, blocker) in cross_pairs {
        out.push(GraphAnomaly::CrossMilestoneBlockedBy { issue, blocker });
    }

    if !parsed.levels.is_empty() {
        let levels_map = parsed.issue_to_level();
        let computed = compute_levels_with(issues, &milestone_set, &blockers_map, &cycles);

        let mut sorted_issues: Vec<&GhIssue> = issues.iter().collect();
        sorted_issues.sort_by_key(|i| i.number);
        for issue in sorted_issues {
            if cycle_members.contains(&issue.number) {
                continue;
            }
            match (levels_map.get(&issue.number), computed.get(&issue.number)) {
                (None, _) => out.push(GraphAnomaly::IssueMissingFromGraph {
                    issue: issue.number,
                }),
                (Some(&found), Some(Some(expected))) if *expected != found => {
                    out.push(GraphAnomaly::WrongLevelAssignment {
                        issue: issue.number,
                        expected: *expected,
                        found,
                    });
                }
                _ => {}
            }
        }

        let mut graph_unknowns: Vec<u64> = parsed
            .all_issues()
            .into_iter()
            .filter(|n| !milestone_set.contains(n))
            .collect();
        graph_unknowns.sort_unstable();
        for n in graph_unknowns {
            out.push(GraphAnomaly::GraphReferencesUnknownIssue { issue: n });
        }
    }

    out
}

#[cfg(test)]
mod tests;
