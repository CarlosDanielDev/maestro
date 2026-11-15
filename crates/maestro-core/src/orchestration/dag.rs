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
#[path = "dag_tests.rs"]
mod tests;
