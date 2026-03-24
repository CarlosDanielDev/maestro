use crate::github::types::{GhIssue, Priority, SessionMode};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    Pending,
    Blocked,
    InProgress,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub priority: Priority,
    pub mode: Option<SessionMode>,
    pub status: WorkStatus,
    /// Cached blockers (computed once from issue labels + body).
    pub blocked_by: Vec<u64>,
    /// The original issue data.
    pub issue: GhIssue,
}

impl WorkItem {
    pub fn from_issue(issue: GhIssue) -> Self {
        let priority = issue.priority();
        let mode = issue.session_mode();
        let blocked_by = issue.all_blockers();

        Self {
            priority,
            mode,
            status: WorkStatus::Pending,
            blocked_by,
            issue,
        }
    }

    /// Convenience accessor for issue number.
    pub fn number(&self) -> u64 {
        self.issue.number
    }

    /// Convenience accessor for issue title.
    pub fn title(&self) -> &str {
        &self.issue.title
    }

    /// A work item is ready if its status is Pending and all blockers are in the completed set.
    pub fn is_ready(&self, completed: &HashSet<u64>) -> bool {
        self.status == WorkStatus::Pending && self.blocked_by.iter().all(|b| completed.contains(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gh_issue(number: u64, labels: &[&str], body: &str) -> GhIssue {
        GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: body.to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: "open".to_string(),
            html_url: format!("https://github.com/o/r/issues/{}", number),
        }
    }

    // WorkItem::from_issue

    #[test]
    fn from_issue_captures_number_and_title() {
        let issue = make_gh_issue(42, &["maestro:ready"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.number(), 42);
        assert_eq!(item.title(), "Issue #42");
    }

    #[test]
    fn from_issue_extracts_p0_priority() {
        let issue = make_gh_issue(1, &["priority:P0", "maestro:ready"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.priority, Priority::P0);
    }

    #[test]
    fn from_issue_extracts_p1_priority() {
        let issue = make_gh_issue(2, &["priority:P1"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.priority, Priority::P1);
    }

    #[test]
    fn from_issue_defaults_priority_to_p2() {
        let issue = make_gh_issue(3, &["maestro:ready"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.priority, Priority::P2);
    }

    #[test]
    fn from_issue_extracts_session_mode_orchestrator() {
        let issue = make_gh_issue(4, &["mode:orchestrator"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.mode, Some(SessionMode::Orchestrator));
    }

    #[test]
    fn from_issue_extracts_session_mode_vibe() {
        let issue = make_gh_issue(5, &["mode:vibe"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.mode, Some(SessionMode::Vibe));
    }

    #[test]
    fn from_issue_mode_none_when_no_mode_label() {
        let issue = make_gh_issue(6, &["maestro:ready"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.mode, None);
    }

    #[test]
    fn from_issue_collects_blockers_from_labels() {
        let issue = make_gh_issue(10, &["blocked-by:#3", "blocked-by:#7"], "");
        let item = WorkItem::from_issue(issue);
        let mut blockers = item.blocked_by.clone();
        blockers.sort();
        assert_eq!(blockers, vec![3u64, 7u64]);
    }

    #[test]
    fn from_issue_collects_blockers_from_body() {
        let issue = make_gh_issue(10, &[], "blocked-by: #5\nblocked-by: #9");
        let item = WorkItem::from_issue(issue);
        let mut blockers = item.blocked_by.clone();
        blockers.sort();
        assert_eq!(blockers, vec![5u64, 9u64]);
    }

    #[test]
    fn from_issue_deduplicates_blockers_from_labels_and_body() {
        let issue = make_gh_issue(10, &["blocked-by:#2"], "blocked-by: #2\nblocked-by: #4");
        let item = WorkItem::from_issue(issue);
        let mut blockers = item.blocked_by.clone();
        blockers.sort();
        assert_eq!(blockers, vec![2u64, 4u64]);
    }

    #[test]
    fn from_issue_initial_status_is_pending() {
        let issue = make_gh_issue(1, &["maestro:ready"], "");
        let item = WorkItem::from_issue(issue);
        assert_eq!(item.status, WorkStatus::Pending);
    }

    // WorkItem::is_ready

    #[test]
    fn is_ready_true_when_pending_and_no_blockers() {
        let issue = make_gh_issue(1, &["maestro:ready"], "");
        let item = WorkItem::from_issue(issue);
        assert!(item.is_ready(&HashSet::new()));
    }

    #[test]
    fn is_ready_false_when_blocked_by_pending_issue() {
        let issue = make_gh_issue(2, &["blocked-by:#1"], "");
        let item = WorkItem::from_issue(issue);
        assert!(!item.is_ready(&HashSet::new()));
    }

    #[test]
    fn is_ready_true_when_all_blockers_completed() {
        let issue = make_gh_issue(2, &["blocked-by:#1"], "");
        let item = WorkItem::from_issue(issue);
        assert!(item.is_ready(&HashSet::from([1u64])));
    }

    #[test]
    fn is_ready_false_when_status_is_not_pending() {
        let issue = make_gh_issue(1, &["maestro:ready"], "");
        let mut item = WorkItem::from_issue(issue);
        item.status = WorkStatus::InProgress;
        assert!(!item.is_ready(&HashSet::new()));
    }

    #[test]
    fn is_ready_false_when_one_of_multiple_blockers_incomplete() {
        let issue = make_gh_issue(5, &["blocked-by:#1", "blocked-by:#2"], "");
        let item = WorkItem::from_issue(issue);
        assert!(!item.is_ready(&HashSet::from([1u64])));
    }

    #[test]
    fn is_ready_true_when_all_multiple_blockers_completed() {
        let issue = make_gh_issue(5, &["blocked-by:#1", "blocked-by:#2"], "");
        let item = WorkItem::from_issue(issue);
        assert!(item.is_ready(&HashSet::from([1u64, 2u64])));
    }
}
