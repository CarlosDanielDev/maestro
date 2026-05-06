//! DAG construction helpers — Blocked-By parsing, edge classification,
//! topological ordering, and bounded auto-expansion. Spec §5.

#![allow(dead_code)]

use crate::state::types::IssueNumber;
use anyhow::{Result, anyhow};
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Parse the `## Blocked By` section of an issue body.
///
/// Handles:
/// - explicit "None" (case-insensitive) → empty vec
/// - single or multiple issue references (`#123` style) in bullets or prose
/// - malformed sections (returns whatever numeric tokens exist, else empty)
/// - missing section → empty vec
pub fn parse_blocked_by(body: &str) -> Vec<IssueNumber> {
    let header_re = Regex::new(r"(?im)^##\s+Blocked\s+By\s*$").unwrap();
    let Some(header) = header_re.find(body) else {
        return Vec::new();
    };

    let rest = &body[header.end()..];
    let next_header_re = Regex::new(r"(?m)^##\s+").unwrap();
    let section = match next_header_re.find(rest) {
        Some(m) => &rest[..m.start()],
        None => rest,
    };

    if section.to_lowercase().contains("none") {
        return Vec::new();
    }

    let issue_re = Regex::new(r"#(\d+)").unwrap();
    issue_re
        .captures_iter(section)
        .filter_map(|c| c.get(1))
        .filter_map(|m| m.as_str().parse::<IssueNumber>().ok())
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Edge {
    InSlice(IssueNumber),
    ClosedExternal(IssueNumber),
    SameMilestoneOpenExternal(IssueNumber),
    CrossMilestoneOpenExternal(IssueNumber),
}

#[derive(Debug, Clone)]
pub struct IssueMeta {
    pub number: IssueNumber,
    pub state: IssueState,
    pub milestone: Option<u64>,
    pub blocked_by: Vec<IssueNumber>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

pub fn classify_edges(
    selected: &HashSet<IssueNumber>,
    primary_milestone: Option<u64>,
    metas: &HashMap<IssueNumber, IssueMeta>,
) -> HashMap<IssueNumber, Vec<Edge>> {
    let mut out = HashMap::new();
    for &issue in selected {
        let Some(meta) = metas.get(&issue) else {
            continue;
        };
        let mut edges = Vec::new();
        for &dep in &meta.blocked_by {
            if selected.contains(&dep) {
                edges.push(Edge::InSlice(dep));
                continue;
            }

            let dep_meta = metas.get(&dep);
            match dep_meta.map(|m| m.state) {
                Some(IssueState::Closed) => edges.push(Edge::ClosedExternal(dep)),
                Some(IssueState::Open) => {
                    let same_ms = match (primary_milestone, dep_meta.unwrap().milestone) {
                        (Some(a), Some(b)) => a == b,
                        _ => false,
                    };
                    edges.push(if same_ms {
                        Edge::SameMilestoneOpenExternal(dep)
                    } else {
                        Edge::CrossMilestoneOpenExternal(dep)
                    });
                }
                None => edges.push(Edge::CrossMilestoneOpenExternal(dep)),
            }
        }
        out.insert(issue, edges);
    }
    out
}

fn detect_cycle(
    remaining: &HashSet<IssueNumber>,
    edges: &HashMap<IssueNumber, Vec<Edge>>,
) -> Option<Vec<IssueNumber>> {
    fn dfs(
        node: IssueNumber,
        edges: &HashMap<IssueNumber, Vec<Edge>>,
        remaining: &HashSet<IssueNumber>,
        visiting: &mut HashSet<IssueNumber>,
        visited: &mut HashSet<IssueNumber>,
        stack: &mut Vec<IssueNumber>,
    ) -> Option<Vec<IssueNumber>> {
        visiting.insert(node);
        stack.push(node);

        if let Some(es) = edges.get(&node) {
            for e in es {
                let Edge::InSlice(dep) = e else {
                    continue;
                };
                if !remaining.contains(dep) {
                    continue;
                }
                if visiting.contains(dep) {
                    // Build cycle path from dep back to dep.
                    let start_idx = stack.iter().position(|n| n == dep).unwrap_or(0);
                    let mut path: Vec<IssueNumber> = stack[start_idx..].to_vec();
                    path.push(*dep);
                    return Some(path);
                }
                if !visited.contains(dep)
                    && let Some(cycle) = dfs(*dep, edges, remaining, visiting, visited, stack)
                {
                    return Some(cycle);
                }
            }
        }

        visiting.remove(&node);
        visited.insert(node);
        stack.pop();
        None
    }

    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();
    let mut stack = Vec::new();

    for &n in remaining {
        if visited.contains(&n) {
            continue;
        }
        if let Some(path) = dfs(n, edges, remaining, &mut visiting, &mut visited, &mut stack) {
            return Some(path);
        }
    }
    None
}

/// Build topological levels via Kahn's algorithm. Each level contains
/// issues whose in-slice dependencies are satisfied by previous levels.
pub fn topo_levels(
    selected: &HashSet<IssueNumber>,
    edges: &HashMap<IssueNumber, Vec<Edge>>,
) -> Result<Vec<Vec<IssueNumber>>> {
    let mut in_degree: HashMap<IssueNumber, u32> = selected.iter().map(|n| (*n, 0)).collect();
    let mut adj: HashMap<IssueNumber, Vec<IssueNumber>> = HashMap::new();

    for (&from, es) in edges {
        for e in es {
            if let Edge::InSlice(dep) = e {
                *in_degree.entry(from).or_insert(0) += 1;
                adj.entry(*dep).or_default().push(from);
            }
        }
    }

    let mut levels = Vec::new();
    let mut remaining: HashSet<IssueNumber> = selected.iter().copied().collect();

    while !remaining.is_empty() {
        let mut level: Vec<IssueNumber> = remaining
            .iter()
            .copied()
            .filter(|n| in_degree.get(n).copied().unwrap_or(0) == 0)
            .collect();
        if level.is_empty() {
            let path = detect_cycle(&remaining, edges)
                .unwrap_or_else(|| remaining.iter().copied().collect());
            return Err(anyhow!("cycle in dependency graph: {:?}", path));
        }
        for n in &level {
            remaining.remove(n);
            if let Some(children) = adj.get(n) {
                for c in children {
                    if let Some(d) = in_degree.get_mut(c) {
                        *d = d.saturating_sub(1);
                    }
                }
            }
        }
        level.sort();
        levels.push(level);
    }

    Ok(levels)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpandResult {
    NoChange {
        selected: HashSet<IssueNumber>,
    },
    Expanded {
        selected: HashSet<IssueNumber>,
        added: Vec<IssueNumber>,
    },
    TooLarge {
        original: usize,
        would_be: usize,
        added: Vec<IssueNumber>,
    },
}

/// Auto-add same-milestone open-external deps to the selection. Single-pass
/// (does not recurse). Refuses if expansion would exceed 2x original count.
pub fn auto_expand(
    selected: HashSet<IssueNumber>,
    edges: &HashMap<IssueNumber, Vec<Edge>>,
) -> ExpandResult {
    let original_count = selected.len();
    let mut expanded = selected.clone();
    let mut added: Vec<IssueNumber> = Vec::new();

    for es in edges.values() {
        for e in es {
            if let Edge::SameMilestoneOpenExternal(dep) = e
                && expanded.insert(*dep)
            {
                added.push(*dep);
            }
        }
    }

    if expanded.len() > original_count.saturating_mul(2) {
        return ExpandResult::TooLarge {
            original: original_count,
            would_be: expanded.len(),
            added,
        };
    }

    if added.is_empty() {
        return ExpandResult::NoChange { selected };
    }

    added.sort_unstable();
    ExpandResult::Expanded {
        selected: expanded,
        added,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_none_fixture() {
        let body = include_str!("../../tests/fixtures/blocked_by/none.md");
        assert!(parse_blocked_by(body).is_empty());
    }

    #[test]
    fn parse_single_fixture() {
        let body = include_str!("../../tests/fixtures/blocked_by/single.md");
        assert_eq!(parse_blocked_by(body), vec![123]);
    }

    #[test]
    fn parse_multi_fixture() {
        let body = include_str!("../../tests/fixtures/blocked_by/multi.md");
        let mut deps = parse_blocked_by(body);
        deps.sort_unstable();
        assert_eq!(deps, vec![10, 20, 30]);
    }

    #[test]
    fn parse_malformed_fixture() {
        let body = include_str!("../../tests/fixtures/blocked_by/malformed.md");
        assert_eq!(parse_blocked_by(body), vec![77]);
    }

    #[test]
    fn parse_missing_fixture() {
        let body = include_str!("../../tests/fixtures/blocked_by/missing.md");
        assert!(parse_blocked_by(body).is_empty());
    }

    fn meta(
        n: IssueNumber,
        state: IssueState,
        milestone: Option<u64>,
        blocked_by: Vec<IssueNumber>,
    ) -> IssueMeta {
        IssueMeta {
            number: n,
            state,
            milestone,
            blocked_by,
        }
    }

    #[test]
    fn classify_in_slice() {
        let mut metas = HashMap::new();
        metas.insert(1, meta(1, IssueState::Open, Some(1), vec![2]));
        metas.insert(2, meta(2, IssueState::Open, Some(1), vec![]));
        let selected = HashSet::from([1u64, 2]);
        let edges = classify_edges(&selected, Some(1), &metas);
        assert_eq!(edges.get(&1), Some(&vec![Edge::InSlice(2)]));
    }

    #[test]
    fn classify_closed_external() {
        let mut metas = HashMap::new();
        metas.insert(1, meta(1, IssueState::Open, Some(1), vec![3]));
        metas.insert(3, meta(3, IssueState::Closed, Some(1), vec![]));
        let selected = HashSet::from([1u64]);
        let edges = classify_edges(&selected, Some(1), &metas);
        assert_eq!(edges.get(&1), Some(&vec![Edge::ClosedExternal(3)]));
    }

    #[test]
    fn classify_same_milestone_open_external() {
        let mut metas = HashMap::new();
        metas.insert(1, meta(1, IssueState::Open, Some(1), vec![4]));
        metas.insert(4, meta(4, IssueState::Open, Some(1), vec![]));
        let selected = HashSet::from([1u64]);
        let edges = classify_edges(&selected, Some(1), &metas);
        assert_eq!(
            edges.get(&1),
            Some(&vec![Edge::SameMilestoneOpenExternal(4)])
        );
    }

    #[test]
    fn classify_cross_milestone_open_external_when_missing_meta() {
        let mut metas = HashMap::new();
        metas.insert(1, meta(1, IssueState::Open, Some(1), vec![9]));
        let selected = HashSet::from([1u64]);
        let edges = classify_edges(&selected, Some(1), &metas);
        assert_eq!(
            edges.get(&1),
            Some(&vec![Edge::CrossMilestoneOpenExternal(9)])
        );
    }

    #[test]
    fn topo_linear_chain() {
        let selected = HashSet::from([1u64, 2, 3]);
        let mut edges = HashMap::new();
        edges.insert(2, vec![Edge::InSlice(1)]);
        edges.insert(3, vec![Edge::InSlice(2)]);
        let levels = topo_levels(&selected, &edges).unwrap();
        assert_eq!(levels, vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn topo_parallel_leaves() {
        let selected = HashSet::from([1u64, 2, 3]);
        let levels = topo_levels(&selected, &HashMap::new()).unwrap();
        assert_eq!(levels.len(), 1);
        let mut l0 = levels[0].clone();
        l0.sort_unstable();
        assert_eq!(l0, vec![1, 2, 3]);
    }

    #[test]
    fn topo_cycle_reports_path() {
        let selected = HashSet::from([1u64, 2]);
        let mut edges = HashMap::new();
        edges.insert(1, vec![Edge::InSlice(2)]);
        edges.insert(2, vec![Edge::InSlice(1)]);
        let err = topo_levels(&selected, &edges).unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn auto_expand_adds_same_milestone() {
        let selected = HashSet::from([1u64, 2]);
        let mut edges = HashMap::new();
        edges.insert(2, vec![Edge::SameMilestoneOpenExternal(3)]);
        let r = auto_expand(selected.clone(), &edges);
        match r {
            ExpandResult::Expanded { selected: s, added } => {
                assert!(s.contains(&3));
                assert_eq!(added, vec![3]);
            }
            _ => panic!("expected Expanded"),
        }
    }

    #[test]
    fn auto_expand_refuses_when_more_than_double() {
        let selected = HashSet::from([1u64]);
        let mut edges = HashMap::new();
        edges.insert(
            1,
            vec![
                Edge::SameMilestoneOpenExternal(2),
                Edge::SameMilestoneOpenExternal(3),
                Edge::SameMilestoneOpenExternal(4),
            ],
        );
        let r = auto_expand(selected, &edges);
        assert!(matches!(r, ExpandResult::TooLarge { .. }));
    }
}
