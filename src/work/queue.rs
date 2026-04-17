use std::collections::{HashSet, VecDeque};
use std::fmt;

use super::dependencies::DependencyGraph;

/// A single item in the validated work queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedItem {
    pub issue_number: u64,
    pub position: usize,
    pub dependencies: Vec<u64>,
}

/// An ordered list of validated, independent work items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueue {
    items: Vec<QueuedItem>,
}

/// Errors that can occur when validating a selection of issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueValidationError {
    /// Two selected issues have a direct or transitive dependency.
    MutualDependency { from: u64, to: u64 },
    /// A dependency cycle exists among the selected issues.
    CycleDetected { issues: Vec<u64> },
}

impl fmt::Display for QueueValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueueValidationError::MutualDependency { from, to } => {
                write!(f, "Issue #{} depends on #{}, both are selected", from, to)
            }
            QueueValidationError::CycleDetected { issues } => {
                let nums: Vec<String> = issues.iter().map(|n| format!("#{}", n)).collect();
                write!(f, "Dependency cycle detected among: {}", nums.join(", "))
            }
        }
    }
}

impl std::error::Error for QueueValidationError {}

impl WorkQueue {
    /// Validate a selection of issue numbers against the dependency graph.
    ///
    /// Returns an ordered `WorkQueue` if all selected issues are independent
    /// of each other (no direct or transitive dependencies between them).
    /// External dependencies (on issues not in the selection) are ignored.
    pub fn validate_selection(
        issues: &[u64],
        graph: &DependencyGraph,
    ) -> Result<WorkQueue, QueueValidationError> {
        if issues.is_empty() {
            return Ok(WorkQueue { items: vec![] });
        }

        let selected: HashSet<u64> = issues.iter().copied().collect();

        // Check for self-loops (issue depends on itself)
        for &issue in &selected {
            if graph.dependents_of(issue).contains(&issue) {
                return Err(QueueValidationError::CycleDetected {
                    issues: vec![issue],
                });
            }
        }

        // For each selected issue, BFS through dependents_of to find
        // which other selected issues are transitively reachable.
        // If issue A can reach issue B via dependents_of, then B depends on A.
        for &issue in &selected {
            let reachable = transitive_dependents(issue, graph, &selected);

            if let Some(&reached) = reachable.iter().next() {
                // Check if the reverse is also true (cycle)
                let reverse = transitive_dependents(reached, graph, &selected);
                if reverse.contains(&issue) {
                    let mut cycle = vec![issue, reached];
                    cycle.sort_unstable();
                    return Err(QueueValidationError::CycleDetected { issues: cycle });
                }

                // One-directional: `reached` depends on `issue`
                return Err(QueueValidationError::MutualDependency {
                    from: reached,
                    to: issue,
                });
            }
        }

        // All independent — build the queue (deduplicated, preserving input order)
        let mut seen = HashSet::new();
        let items = issues
            .iter()
            .filter(|n| seen.insert(**n))
            .enumerate()
            .map(|(pos, &num)| QueuedItem {
                issue_number: num,
                position: pos,
                dependencies: vec![],
            })
            .collect();

        Ok(WorkQueue { items })
    }

    /// Returns the items in the queue.
    pub fn items(&self) -> &[QueuedItem] {
        &self.items
    }

    /// Returns the number of items in the queue.
    #[allow(dead_code)] // Reason: standard collection API surface
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns true if the queue is empty.
    #[allow(dead_code)] // Reason: standard collection API surface
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// BFS through `dependents_of` starting from `start`, returning all
/// selected issues that are transitively reachable (i.e., depend on `start`).
fn transitive_dependents(
    start: u64,
    graph: &DependencyGraph,
    selected: &HashSet<u64>,
) -> HashSet<u64> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut found = HashSet::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        for dep in graph.dependents_of(current) {
            if !visited.insert(dep) {
                continue;
            }
            if selected.contains(&dep) {
                found.insert(dep);
            }
            queue.push_back(dep);
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::types::GhIssue;
    use crate::work::types::WorkItem;

    fn make_item(number: u64, blocked_by: &[u64]) -> WorkItem {
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

    fn build_graph(items: &[WorkItem]) -> DependencyGraph {
        DependencyGraph::build(items)
    }

    // --- Empty selection ---

    #[test]
    fn empty_selection_returns_empty_queue() {
        let graph = build_graph(&[]);
        let result = WorkQueue::validate_selection(&[], &graph);
        let queue = result.unwrap();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    // --- Single issue ---

    #[test]
    fn single_issue_always_succeeds() {
        let items = vec![make_item(1, &[])];
        let graph = build_graph(&items);
        let queue = WorkQueue::validate_selection(&[1], &graph).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.items()[0].issue_number, 1);
        assert_eq!(queue.items()[0].position, 0);
    }

    #[test]
    fn single_issue_with_external_deps_succeeds() {
        // Issue 5 depends on 3, but 3 is not in the selection
        let items = vec![make_item(5, &[3]), make_item(3, &[])];
        let graph = build_graph(&items);
        let queue = WorkQueue::validate_selection(&[5], &graph).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.items()[0].issue_number, 5);
    }

    // --- Independent issues ---

    #[test]
    fn three_independent_issues_returns_all() {
        let items = vec![make_item(1, &[]), make_item(2, &[]), make_item(3, &[])];
        let graph = build_graph(&items);
        let queue = WorkQueue::validate_selection(&[1, 2, 3], &graph).unwrap();
        assert_eq!(queue.len(), 3);

        let numbers: Vec<u64> = queue.items().iter().map(|i| i.issue_number).collect();
        assert!(numbers.contains(&1));
        assert!(numbers.contains(&2));
        assert!(numbers.contains(&3));
    }

    #[test]
    fn independent_issues_have_sequential_positions() {
        let items = vec![make_item(10, &[]), make_item(20, &[]), make_item(30, &[])];
        let graph = build_graph(&items);
        let queue = WorkQueue::validate_selection(&[10, 20, 30], &graph).unwrap();

        let positions: Vec<usize> = queue.items().iter().map(|i| i.position).collect();
        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn independent_issues_have_empty_dependencies() {
        let items = vec![make_item(1, &[]), make_item(2, &[])];
        let graph = build_graph(&items);
        let queue = WorkQueue::validate_selection(&[1, 2], &graph).unwrap();

        for item in queue.items() {
            assert!(item.dependencies.is_empty());
        }
    }

    // --- External deps are ignored ---

    #[test]
    fn issues_with_external_deps_only_are_independent() {
        // Issue 10 depends on 1, issue 20 depends on 2
        // But 1 and 2 are not in the selection — only 10 and 20 are
        let items = vec![
            make_item(1, &[]),
            make_item(2, &[]),
            make_item(10, &[1]),
            make_item(20, &[2]),
        ];
        let graph = build_graph(&items);
        let queue = WorkQueue::validate_selection(&[10, 20], &graph).unwrap();
        assert_eq!(queue.len(), 2);
    }

    // --- Direct dependency (MutualDependency) ---

    #[test]
    fn two_issues_with_direct_dependency_returns_error() {
        // Issue 2 depends on issue 1
        let items = vec![make_item(1, &[]), make_item(2, &[1])];
        let graph = build_graph(&items);
        let result = WorkQueue::validate_selection(&[1, 2], &graph);
        assert!(result.is_err());

        match result.unwrap_err() {
            QueueValidationError::MutualDependency { from, to } => {
                assert_eq!(from, 2);
                assert_eq!(to, 1);
            }
            other => panic!("Expected MutualDependency, got {:?}", other),
        }
    }

    // --- Transitive dependency ---

    #[test]
    fn transitive_dependency_a_to_c_via_b_rejected() {
        // Chain: 1 <- 2 <- 3 (3 depends on 2, 2 depends on 1)
        // Select 1 and 3: transitive dependency through 2
        let items = vec![make_item(1, &[]), make_item(2, &[1]), make_item(3, &[2])];
        let graph = build_graph(&items);
        let result = WorkQueue::validate_selection(&[1, 3], &graph);
        assert!(result.is_err());

        match result.unwrap_err() {
            QueueValidationError::MutualDependency { from, to } => {
                assert_eq!(from, 3);
                assert_eq!(to, 1);
            }
            other => panic!("Expected MutualDependency, got {:?}", other),
        }
    }

    // --- Cycle detection ---

    #[test]
    fn direct_cycle_returns_cycle_detected() {
        // 1 depends on 2, 2 depends on 1
        let items = vec![make_item(1, &[2]), make_item(2, &[1])];
        let graph = build_graph(&items);
        let result = WorkQueue::validate_selection(&[1, 2], &graph);
        assert!(result.is_err());

        match result.unwrap_err() {
            QueueValidationError::CycleDetected { issues } => {
                assert!(issues.contains(&1));
                assert!(issues.contains(&2));
            }
            other => panic!("Expected CycleDetected, got {:?}", other),
        }
    }

    #[test]
    fn three_node_cycle_returns_cycle_detected() {
        // 1 -> 3 -> 2 -> 1
        let items = vec![make_item(1, &[3]), make_item(2, &[1]), make_item(3, &[2])];
        let graph = build_graph(&items);
        let result = WorkQueue::validate_selection(&[1, 2, 3], &graph);
        assert!(result.is_err());

        match result.unwrap_err() {
            QueueValidationError::CycleDetected { .. } => {}
            other => panic!("Expected CycleDetected, got {:?}", other),
        }
    }

    // --- Edge cases ---

    #[test]
    fn self_referential_issue_returns_error() {
        // Issue 7 blocked by itself — self-cycle
        let items = vec![make_item(7, &[7])];
        let graph = build_graph(&items);
        let result = WorkQueue::validate_selection(&[7], &graph);
        assert!(result.is_err());
    }

    #[test]
    fn selection_with_issue_not_in_graph_succeeds() {
        // Issue 999 was never in the graph — no edges, treated as independent
        let items = vec![make_item(1, &[])];
        let graph = build_graph(&items);
        let result = WorkQueue::validate_selection(&[1, 999], &graph).unwrap();
        assert_eq!(result.len(), 2);
    }

    // --- Error display ---

    #[test]
    fn mutual_dependency_error_displays_correctly() {
        let err = QueueValidationError::MutualDependency { from: 5, to: 3 };
        let msg = format!("{}", err);
        assert!(msg.contains("#5"));
        assert!(msg.contains("#3"));
    }

    #[test]
    fn cycle_detected_error_displays_correctly() {
        let err = QueueValidationError::CycleDetected {
            issues: vec![1, 2, 3],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("#1"));
        assert!(msg.contains("#2"));
        assert!(msg.contains("#3"));
        assert!(msg.contains("cycle"));
    }
}
