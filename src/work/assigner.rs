use super::dependencies::DependencyGraph;
use super::types::{WorkItem, WorkStatus};
use std::collections::{HashMap, HashSet};

/// Counts of work items by status.
pub struct StatusCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub done: usize,
    pub failed: usize,
}

/// Manages the work queue: priority ordering, dependency resolution,
/// and assignment of issues to session slots.
pub struct WorkAssigner {
    items: Vec<WorkItem>,
    completed_issues: HashSet<u64>,
    /// Topological order index for tiebreaking (lower = earlier in dependency chain).
    topo_order: HashMap<u64, usize>,
    /// Cached dependency graph for cascade operations.
    graph: DependencyGraph,
}

impl WorkAssigner {
    /// Create a new WorkAssigner, running cycle detection on the dependency graph.
    /// Items involved in cycles are immediately marked as Failed.
    pub fn new(mut items: Vec<WorkItem>) -> Self {
        let graph = DependencyGraph::build(&items);
        let topo_order = match graph.topological_sort() {
            Ok(order) => {
                let map: HashMap<u64, usize> = order
                    .into_iter()
                    .enumerate()
                    .map(|(idx, num)| (num, idx))
                    .collect();
                map
            }
            Err(_) => {
                // Cycle detected — do a partial sort to identify which items are in cycles
                // Items not in the successful partial order are cycling
                let partial = Self::partial_topo_sort(&graph, &items);
                let ordered: HashSet<u64> = partial.keys().copied().collect();
                for item in &mut items {
                    if !ordered.contains(&item.number()) {
                        item.status = WorkStatus::Failed;
                    }
                }
                partial
            }
        };

        Self {
            graph,
            items,
            completed_issues: HashSet::new(),
            topo_order,
        }
    }

    /// Get the next batch of ready work items (up to `count`),
    /// sorted by priority (P0 first), then by topo order, then by issue number.
    pub fn next_ready(&self, count: usize) -> Vec<&WorkItem> {
        let mut ready: Vec<&WorkItem> = self
            .items
            .iter()
            .filter(|item| item.is_ready(&self.completed_issues))
            .collect();

        ready.sort_by(|a, b| {
            a.priority.cmp(&b.priority).then_with(|| {
                let a_topo = self
                    .topo_order
                    .get(&a.number())
                    .copied()
                    .unwrap_or(usize::MAX);
                let b_topo = self
                    .topo_order
                    .get(&b.number())
                    .copied()
                    .unwrap_or(usize::MAX);
                a_topo
                    .cmp(&b_topo)
                    .then_with(|| a.number().cmp(&b.number()))
            })
        });

        ready.into_iter().take(count).collect()
    }

    /// Get issues that were detected as part of a dependency cycle (marked Failed at init).
    pub fn cycling_issues(&self) -> Vec<u64> {
        let ordered: HashSet<u64> = self.topo_order.keys().copied().collect();
        self.items
            .iter()
            .filter(|i| !ordered.contains(&i.number()) && i.status == WorkStatus::Failed)
            .map(|i| i.number())
            .collect()
    }

    /// Build a partial topological order, skipping cycle nodes.
    fn partial_topo_sort(graph: &DependencyGraph, items: &[WorkItem]) -> HashMap<u64, usize> {
        // Try ordering each item independently — items reachable from roots get ordered
        let mut in_degree: HashMap<u64, usize> = HashMap::new();
        let all_nums: HashSet<u64> = items.iter().map(|i| i.number()).collect();

        for item in items {
            in_degree.entry(item.number()).or_insert(0);
            let dep_count = item
                .blocked_by
                .iter()
                .filter(|b| all_nums.contains(b))
                .count();
            *in_degree.entry(item.number()).or_insert(0) = dep_count;
        }

        let mut queue: std::collections::VecDeque<u64> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&n, _)| n)
            .collect();

        let mut result = HashMap::new();
        let mut idx = 0;

        while let Some(node) = queue.pop_front() {
            result.insert(node, idx);
            idx += 1;
            for dep in graph.dependents_of(node) {
                if let Some(deg) = in_degree.get_mut(&dep) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }

        result
    }

    /// Mark an issue as in-progress.
    pub fn mark_in_progress(&mut self, issue_number: u64) {
        if let Some(item) = self.items.iter_mut().find(|i| i.number() == issue_number) {
            item.status = WorkStatus::InProgress;
        }
    }

    /// Mark an issue as completed. Returns newly unblocked work items.
    pub fn mark_done(&mut self, issue_number: u64) -> Vec<&WorkItem> {
        self.completed_issues.insert(issue_number);
        if let Some(item) = self.items.iter_mut().find(|i| i.number() == issue_number) {
            item.status = WorkStatus::Done;
        }
        self.get_newly_unblocked()
    }

    /// Mark an issue as failed.
    pub fn mark_failed(&mut self, issue_number: u64) {
        if let Some(item) = self.items.iter_mut().find(|i| i.number() == issue_number) {
            item.status = WorkStatus::Failed;
        }
    }

    /// Mark an issue as failed AND cascade failure to all transitive dependents.
    /// Returns the list of issue numbers that were cascade-failed.
    pub fn mark_failed_cascade(&mut self, issue_number: u64) -> Vec<u64> {
        self.mark_failed(issue_number);

        // BFS to find all transitive dependents using cached graph
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(issue_number);

        while let Some(current) = queue.pop_front() {
            for dep in self.graph.dependents_of(current) {
                if dep != issue_number && visited.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }

        // Mark all transitive dependents as failed
        for &num in &visited {
            if let Some(item) = self.items.iter_mut().find(|i| i.number() == num)
                && !matches!(item.status, WorkStatus::Done | WorkStatus::Failed)
            {
                item.status = WorkStatus::Failed;
            }
        }

        visited.into_iter().collect()
    }

    /// Get all work items for display.
    pub fn all_items(&self) -> &[WorkItem] {
        &self.items
    }

    /// Count of items by status.
    pub fn count_by_status(&self) -> StatusCounts {
        let mut counts = StatusCounts {
            pending: 0,
            in_progress: 0,
            done: 0,
            failed: 0,
        };
        for item in &self.items {
            match item.status {
                WorkStatus::Pending | WorkStatus::Blocked => counts.pending += 1,
                WorkStatus::InProgress => counts.in_progress += 1,
                WorkStatus::Done => counts.done += 1,
                WorkStatus::Failed => counts.failed += 1,
            }
        }
        counts
    }

    /// Total items in the assigner.
    pub fn total(&self) -> usize {
        self.items.len()
    }

    /// Check if all items are in terminal states (Done or Failed).
    pub fn all_terminal(&self) -> bool {
        self.items
            .iter()
            .all(|i| matches!(i.status, WorkStatus::Done | WorkStatus::Failed))
    }

    /// Find items that just became ready after a completion.
    fn get_newly_unblocked(&self) -> Vec<&WorkItem> {
        self.items
            .iter()
            .filter(|item| item.is_ready(&self.completed_issues))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::types::{GhIssue, Priority};

    fn make_item(number: u64, priority: Priority, blocked_by: &[u64]) -> WorkItem {
        let mut labels: Vec<String> = blocked_by
            .iter()
            .map(|b| format!("blocked-by:#{}", b))
            .collect();
        // Encode priority as a label
        labels.push(format!("priority:{:?}", priority));
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

    fn assigner_from(items: Vec<WorkItem>) -> WorkAssigner {
        WorkAssigner::new(items)
    }

    // WorkAssigner::new

    #[test]
    fn new_accepts_empty_items() {
        let assigner = assigner_from(vec![]);
        assert_eq!(assigner.total(), 0);
    }

    #[test]
    fn total_reflects_all_items() {
        let assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
        ]);
        assert_eq!(assigner.total(), 2);
    }

    // next_ready — priority ordering

    #[test]
    fn next_ready_returns_p0_before_p1_before_p2() {
        let assigner = assigner_from(vec![
            make_item(3, Priority::P2, &[]),
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
        ]);
        let ready = assigner.next_ready(3);
        assert_eq!(ready[0].number(), 1);
        assert_eq!(ready[1].number(), 2);
        assert_eq!(ready[2].number(), 3);
    }

    #[test]
    fn next_ready_respects_count_limit() {
        let assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P0, &[]),
            make_item(3, Priority::P0, &[]),
        ]);
        let ready = assigner.next_ready(2);
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn next_ready_returns_empty_when_all_blocked() {
        let assigner = assigner_from(vec![
            make_item(2, Priority::P1, &[1]),
            make_item(3, Priority::P0, &[1]),
        ]);
        let ready = assigner.next_ready(10);
        assert!(ready.is_empty());
    }

    #[test]
    fn next_ready_excludes_in_progress_items() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
        ]);
        assigner.mark_in_progress(1);
        let ready = assigner.next_ready(10);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].number(), 2);
    }

    #[test]
    fn next_ready_excludes_done_items() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
        ]);
        assigner.mark_in_progress(1);
        assigner.mark_done(1);
        let ready = assigner.next_ready(10);
        assert!(ready.iter().all(|i| i.number() != 1));
    }

    #[test]
    fn next_ready_returns_items_unblocked_after_dependency_done() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[1]),
        ]);
        assert!(assigner.next_ready(10).iter().all(|i| i.number() != 2));

        assigner.mark_in_progress(1);
        assigner.mark_done(1);

        let ready = assigner.next_ready(10);
        assert!(ready.iter().any(|i| i.number() == 2));
    }

    // mark_in_progress

    #[test]
    fn mark_in_progress_changes_status() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(1);
        let counts = assigner.count_by_status();
        assert_eq!(counts.in_progress, 1);
        assert_eq!(counts.pending, 0);
    }

    #[test]
    fn mark_in_progress_unknown_id_is_noop() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(999);
        assert_eq!(assigner.count_by_status().pending, 1);
    }

    // mark_done

    #[test]
    fn mark_done_returns_newly_unblocked_items() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[1]),
            make_item(3, Priority::P2, &[1]),
        ]);
        assigner.mark_in_progress(1);
        let unblocked = assigner.mark_done(1);
        let mut nums: Vec<u64> = unblocked.iter().map(|i| i.number()).collect();
        nums.sort();
        assert_eq!(nums, vec![2, 3]);
    }

    #[test]
    fn mark_done_returns_empty_when_no_dependents() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(1);
        let unblocked = assigner.mark_done(1);
        assert!(unblocked.is_empty());
    }

    #[test]
    fn mark_done_only_unblocks_items_with_all_deps_satisfied() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P0, &[]),
            make_item(3, Priority::P1, &[1, 2]),
        ]);
        assigner.mark_in_progress(1);
        let unblocked = assigner.mark_done(1);
        assert!(unblocked.iter().all(|i| i.number() != 3));
    }

    #[test]
    fn mark_done_updates_status_count() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(1);
        assigner.mark_done(1);
        let counts = assigner.count_by_status();
        assert_eq!(counts.done, 1);
        assert_eq!(counts.in_progress, 0);
    }

    // mark_failed

    #[test]
    fn mark_failed_changes_status_to_failed() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(1);
        assigner.mark_failed(1);
        let counts = assigner.count_by_status();
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.in_progress, 0);
    }

    #[test]
    fn mark_failed_unknown_id_is_noop() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_failed(999);
        assert_eq!(assigner.count_by_status().pending, 1);
    }

    // mark_failed_cascade

    #[test]
    fn mark_failed_cascade_marks_direct_dependents() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[1]),
            make_item(3, Priority::P2, &[1]),
        ]);
        assigner.mark_in_progress(1);
        let cascaded = assigner.mark_failed_cascade(1);
        let mut nums = cascaded.clone();
        nums.sort();
        assert_eq!(nums, vec![2, 3]);
        assert_eq!(assigner.count_by_status().failed, 3);
    }

    #[test]
    fn mark_failed_cascade_marks_transitive_dependents() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[1]),
            make_item(3, Priority::P2, &[2]),
        ]);
        assigner.mark_in_progress(1);
        let cascaded = assigner.mark_failed_cascade(1);
        let mut nums = cascaded.clone();
        nums.sort();
        assert_eq!(nums, vec![2, 3]);
    }

    #[test]
    fn mark_failed_cascade_returns_empty_when_no_dependents() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(1);
        let cascaded = assigner.mark_failed_cascade(1);
        assert!(cascaded.is_empty());
    }

    #[test]
    fn mark_failed_cascade_does_not_cascade_done_items() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[1]),
        ]);
        assigner.mark_in_progress(2);
        assigner.mark_done(2);
        assigner.mark_in_progress(1);
        let cascaded = assigner.mark_failed_cascade(1);
        // Item 2 is already done, should not be cascade-failed
        assert!(
            cascaded.is_empty()
                || !cascaded.contains(&2)
                || assigner
                    .items
                    .iter()
                    .find(|i| i.number() == 2)
                    .unwrap()
                    .status
                    == WorkStatus::Done
        );
    }

    // all_terminal

    #[test]
    fn all_terminal_true_when_all_done() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
        ]);
        assigner.mark_in_progress(1);
        assigner.mark_done(1);
        assigner.mark_in_progress(2);
        assigner.mark_done(2);
        assert!(assigner.all_terminal());
    }

    #[test]
    fn all_terminal_true_when_mix_of_done_and_failed() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
        ]);
        assigner.mark_in_progress(1);
        assigner.mark_done(1);
        assigner.mark_in_progress(2);
        assigner.mark_failed(2);
        assert!(assigner.all_terminal());
    }

    #[test]
    fn all_terminal_false_when_pending_items_remain() {
        let assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assert!(!assigner.all_terminal());
    }

    #[test]
    fn all_terminal_false_when_in_progress_items_remain() {
        let mut assigner = assigner_from(vec![make_item(1, Priority::P0, &[])]);
        assigner.mark_in_progress(1);
        assert!(!assigner.all_terminal());
    }

    #[test]
    fn all_terminal_true_when_empty() {
        let assigner = assigner_from(vec![]);
        assert!(assigner.all_terminal());
    }

    // count_by_status

    #[test]
    fn count_by_status_initial_all_pending() {
        let assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
            make_item(3, Priority::P2, &[]),
        ]);
        let counts = assigner.count_by_status();
        assert_eq!(counts.pending, 3);
        assert_eq!(counts.in_progress, 0);
        assert_eq!(counts.done, 0);
        assert_eq!(counts.failed, 0);
    }

    #[test]
    fn count_by_status_reflects_transitions() {
        let mut assigner = assigner_from(vec![
            make_item(1, Priority::P0, &[]),
            make_item(2, Priority::P1, &[]),
            make_item(3, Priority::P2, &[]),
        ]);
        assigner.mark_in_progress(1);
        assigner.mark_done(1);
        assigner.mark_in_progress(2);
        assigner.mark_failed(2);

        let counts = assigner.count_by_status();
        assert_eq!(counts.pending, 1);
        assert_eq!(counts.in_progress, 0);
        assert_eq!(counts.done, 1);
        assert_eq!(counts.failed, 1);
    }
}
