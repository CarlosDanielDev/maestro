#![allow(dead_code)] // Reason: dependency resolution for work queue — to be wired into orchestration
use std::collections::{HashMap, HashSet, VecDeque};

use super::types::WorkItem;

/// Directed acyclic graph for issue dependencies.
pub struct DependencyGraph {
    /// issue_number -> set of issue numbers it depends on
    edges: HashMap<u64, HashSet<u64>>,
    /// issue_number -> set of issue numbers that depend on it (reverse)
    dependents: HashMap<u64, HashSet<u64>>,
}

impl DependencyGraph {
    /// Build the graph from a slice of work items.
    pub fn build(items: &[WorkItem]) -> Self {
        let mut edges: HashMap<u64, HashSet<u64>> = HashMap::new();
        let mut dependents: HashMap<u64, HashSet<u64>> = HashMap::new();

        let known_issues: HashSet<u64> = items.iter().map(|w| w.number()).collect();

        for item in items {
            let num = item.number();
            let blockers: HashSet<u64> = item
                .blocked_by
                .iter()
                .copied()
                .filter(|b| known_issues.contains(b))
                .collect();

            for &blocker in &blockers {
                dependents.entry(blocker).or_default().insert(num);
            }
            edges.insert(num, blockers);
        }

        Self { edges, dependents }
    }

    /// Return issues in topological order (dependencies first).
    /// Returns Err if a cycle is detected.
    pub fn topological_sort(&self) -> anyhow::Result<Vec<u64>> {
        let mut in_degree: HashMap<u64, usize> = HashMap::new();

        for (&node, deps) in &self.edges {
            in_degree.entry(node).or_insert(0);
            let count = deps.iter().filter(|d| self.edges.contains_key(d)).count();
            *in_degree.entry(node).or_insert(0) = count;
        }

        let mut queue: VecDeque<u64> = VecDeque::new();
        let mut initial: Vec<u64> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&node, _)| node)
            .collect();
        initial.sort_unstable();
        queue.extend(initial);

        let mut result = Vec::new();

        while let Some(node) = queue.pop_front() {
            result.push(node);
            if let Some(deps) = self.dependents.get(&node) {
                let mut sorted_deps: Vec<u64> = deps.iter().copied().collect();
                sorted_deps.sort_unstable();
                for dep in sorted_deps {
                    if let Some(deg) = in_degree.get_mut(&dep) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        if result.len() != self.edges.len() {
            anyhow::bail!(
                "Dependency cycle detected. {} issues could not be ordered.",
                self.edges.len() - result.len()
            );
        }

        Ok(result)
    }

    /// Get the set of issues that depend on a given issue number.
    pub fn dependents_of(&self, issue_number: u64) -> Vec<u64> {
        self.dependents
            .get(&issue_number)
            .map(|s| {
                let mut v: Vec<u64> = s.iter().copied().collect();
                v.sort_unstable();
                v
            })
            .unwrap_or_default()
    }

    /// Check if a work item has unresolved dependencies.
    pub fn has_unresolved_deps(item: &WorkItem, completed: &HashSet<u64>) -> bool {
        item.blocked_by.iter().any(|d| !completed.contains(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(number: u64, blocked_by: &[u64]) -> WorkItem {
        use crate::github::types::GhIssue;
        let labels: Vec<String> = blocked_by
            .iter()
            .map(|b| format!("blocked-by:#{}", b))
            .collect();
        WorkItem::from_issue(GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels,
            state: "open".to_string(),
            html_url: String::new(),
            milestone: None,
            assignees: vec![],
        })
    }

    // DependencyGraph::build

    #[test]
    fn build_creates_graph_with_correct_edges() {
        let items = vec![make_item(1, &[]), make_item(2, &[1]), make_item(3, &[1, 2])];
        let graph = DependencyGraph::build(&items);
        assert!(graph.dependents_of(1).contains(&2));
        assert!(graph.dependents_of(1).contains(&3));
        assert!(graph.dependents_of(2).contains(&3));
    }

    #[test]
    fn build_empty_items_produces_empty_graph() {
        let graph = DependencyGraph::build(&[]);
        assert!(graph.topological_sort().unwrap().is_empty());
    }

    // topological_sort — happy path

    #[test]
    fn topological_sort_single_item_no_deps() {
        let items = vec![make_item(1, &[])];
        let graph = DependencyGraph::build(&items);
        let order = graph.topological_sort().unwrap();
        assert_eq!(order, vec![1]);
    }

    #[test]
    fn topological_sort_linear_chain() {
        let items = vec![make_item(3, &[2]), make_item(2, &[1]), make_item(1, &[])];
        let graph = DependencyGraph::build(&items);
        let order = graph.topological_sort().unwrap();

        let pos = |n: u64| order.iter().position(|&x| x == n).unwrap();
        assert!(pos(1) < pos(2));
        assert!(pos(2) < pos(3));
    }

    #[test]
    fn topological_sort_diamond_dependency() {
        let items = vec![
            make_item(1, &[]),
            make_item(2, &[1]),
            make_item(3, &[1]),
            make_item(4, &[2, 3]),
        ];
        let graph = DependencyGraph::build(&items);
        let order = graph.topological_sort().unwrap();

        let pos = |n: u64| order.iter().position(|&x| x == n).unwrap();
        assert!(pos(1) < pos(2));
        assert!(pos(1) < pos(3));
        assert!(pos(2) < pos(4));
        assert!(pos(3) < pos(4));
    }

    #[test]
    fn topological_sort_parallel_independent_items() {
        let items = vec![make_item(10, &[]), make_item(20, &[]), make_item(30, &[])];
        let graph = DependencyGraph::build(&items);
        let order = graph.topological_sort().unwrap();
        assert_eq!(order.len(), 3);
    }

    // topological_sort — cycle detection

    #[test]
    fn topological_sort_direct_cycle_returns_err() {
        let items = vec![make_item(1, &[2]), make_item(2, &[1])];
        let graph = DependencyGraph::build(&items);
        assert!(
            graph.topological_sort().is_err(),
            "cycle must be detected and return Err"
        );
    }

    #[test]
    fn topological_sort_three_node_cycle_returns_err() {
        let items = vec![make_item(1, &[3]), make_item(2, &[1]), make_item(3, &[2])];
        let graph = DependencyGraph::build(&items);
        assert!(graph.topological_sort().is_err());
    }

    // dependents_of

    #[test]
    fn dependents_of_returns_correct_reverse_lookup() {
        let items = vec![make_item(1, &[]), make_item(2, &[1]), make_item(3, &[1])];
        let graph = DependencyGraph::build(&items);
        let mut deps = graph.dependents_of(1);
        deps.sort();
        assert_eq!(deps, vec![2, 3]);
    }

    #[test]
    fn dependents_of_leaf_node_returns_empty() {
        let items = vec![make_item(1, &[]), make_item(2, &[1])];
        let graph = DependencyGraph::build(&items);
        assert!(graph.dependents_of(2).is_empty());
    }

    #[test]
    fn dependents_of_unknown_node_returns_empty() {
        let graph = DependencyGraph::build(&[]);
        assert!(graph.dependents_of(999).is_empty());
    }

    // has_unresolved_deps

    #[test]
    fn has_unresolved_deps_false_when_no_blockers() {
        let item = make_item(1, &[]);
        let completed: HashSet<u64> = HashSet::new();
        assert!(!DependencyGraph::has_unresolved_deps(&item, &completed));
    }

    #[test]
    fn has_unresolved_deps_false_when_all_blockers_completed() {
        let item = make_item(3, &[1, 2]);
        let completed: HashSet<u64> = [1, 2].into();
        assert!(!DependencyGraph::has_unresolved_deps(&item, &completed));
    }

    #[test]
    fn has_unresolved_deps_true_when_one_blocker_incomplete() {
        let item = make_item(3, &[1, 2]);
        let completed: HashSet<u64> = [1].into();
        assert!(DependencyGraph::has_unresolved_deps(&item, &completed));
    }

    #[test]
    fn has_unresolved_deps_true_when_no_completions() {
        let item = make_item(2, &[1]);
        let completed: HashSet<u64> = HashSet::new();
        assert!(DependencyGraph::has_unresolved_deps(&item, &completed));
    }
}
