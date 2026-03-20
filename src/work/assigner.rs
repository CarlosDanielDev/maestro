use super::types::{WorkItem, WorkStatus};
use std::collections::HashSet;

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
}

impl WorkAssigner {
    pub fn new(items: Vec<WorkItem>) -> Self {
        Self {
            items,
            completed_issues: HashSet::new(),
        }
    }

    /// Get the next batch of ready work items (up to `count`),
    /// sorted by priority (P0 first), then by issue number.
    pub fn next_ready(&self, count: usize) -> Vec<&WorkItem> {
        let completed: Vec<u64> = self.completed_issues.iter().copied().collect();
        let mut ready: Vec<&WorkItem> = self
            .items
            .iter()
            .filter(|item| item.is_ready(&completed))
            .collect();

        ready.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.number().cmp(&b.number()))
        });

        ready.into_iter().take(count).collect()
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
        let completed: Vec<u64> = self.completed_issues.iter().copied().collect();
        self.items
            .iter()
            .filter(|item| item.is_ready(&completed) && item.status == WorkStatus::Pending)
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
