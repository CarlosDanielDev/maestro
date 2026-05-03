#![allow(dead_code)] // Reason: work executor for session orchestration — to be wired into main pipeline
use super::queue::{QueuedItem, WorkQueue};
use anyhow::{Result, bail};

/// Execution state of a single queue item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueItemState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

/// A tracked item inside the executor, combining queue position with runtime state.
#[derive(Debug, Clone)]
pub struct ExecutorItem {
    pub queued: QueuedItem,
    pub state: QueueItemState,
    pub session_id: Option<uuid::Uuid>,
}

/// User decision when a queue item fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureAction {
    Retry,
    Skip,
    Abort,
}

/// The overall state of the queue executor state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorPhase {
    /// No item is currently executing; ready to advance.
    Idle,
    /// A session is running for the current item.
    Running { current_index: usize },
    /// The current item failed; awaiting user decision.
    AwaitingDecision { failed_index: usize },
    /// All items processed (or aborted).
    Finished,
}

/// Manages sequential execution of a validated WorkQueue.
pub struct QueueExecutor {
    items: Vec<ExecutorItem>,
    phase: ExecutorPhase,
}

impl QueueExecutor {
    /// Create an executor from a confirmed WorkQueue.
    pub fn new(queue: &WorkQueue) -> Self {
        let items = queue
            .items()
            .iter()
            .cloned()
            .map(|queued| ExecutorItem {
                queued,
                state: QueueItemState::Pending,
                session_id: None,
            })
            .collect();

        Self {
            items,
            phase: ExecutorPhase::Idle,
        }
    }

    /// Current execution phase.
    pub fn phase(&self) -> ExecutorPhase {
        self.phase
    }

    /// All tracked items (for progress bar rendering).
    pub fn items(&self) -> &[ExecutorItem] {
        &self.items
    }

    /// Number of completed items (Succeeded + Skipped).
    pub fn completed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.state, QueueItemState::Succeeded | QueueItemState::Skipped))
            .count()
    }

    /// Total number of items.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Advance to the next pending item. Returns the QueuedItem to launch,
    /// or None if all items are done.
    pub fn advance(&mut self) -> Result<Option<&QueuedItem>> {
        if !matches!(self.phase, ExecutorPhase::Idle) {
            bail!(
                "cannot advance queue while executor is in {:?} phase",
                self.phase
            );
        }

        let next = self
            .items
            .iter()
            .position(|i| i.state == QueueItemState::Pending);

        match next {
            Some(idx) => {
                self.items[idx].state = QueueItemState::Running;
                self.phase = ExecutorPhase::Running { current_index: idx };
                Ok(Some(&self.items[idx].queued))
            }
            None => {
                self.phase = ExecutorPhase::Finished;
                Ok(None)
            }
        }
    }

    /// Mark the currently running item as having a session ID.
    pub fn set_session_id(&mut self, session_id: uuid::Uuid) {
        if let ExecutorPhase::Running { current_index } = self.phase {
            self.items[current_index].session_id = Some(session_id);
        }
    }

    /// Notify the executor that the current session completed successfully.
    pub fn mark_success(&mut self) {
        if let ExecutorPhase::Running { current_index } = self.phase {
            self.items[current_index].state = QueueItemState::Succeeded;
            self.phase = if self.has_remaining() {
                ExecutorPhase::Idle
            } else {
                ExecutorPhase::Finished
            };
        }
    }

    /// Notify the executor that the current session failed.
    pub fn mark_failure(&mut self) {
        if let ExecutorPhase::Running { current_index } = self.phase {
            self.items[current_index].state = QueueItemState::Failed;
            self.phase = ExecutorPhase::AwaitingDecision {
                failed_index: current_index,
            };
        }
    }

    /// Apply a user decision on the failed item.
    pub fn apply_decision(&mut self, action: FailureAction) {
        if let ExecutorPhase::AwaitingDecision { failed_index } = self.phase {
            match action {
                FailureAction::Retry => {
                    self.items[failed_index].state = QueueItemState::Pending;
                    self.items[failed_index].session_id = None;
                    self.phase = ExecutorPhase::Idle;
                }
                FailureAction::Skip => {
                    self.items[failed_index].state = QueueItemState::Skipped;
                    self.phase = if self.has_remaining() {
                        ExecutorPhase::Idle
                    } else {
                        ExecutorPhase::Finished
                    };
                }
                FailureAction::Abort => {
                    self.phase = ExecutorPhase::Finished;
                }
            }
        }
    }

    /// Whether all items are Succeeded/Skipped (or aborted).
    pub fn is_finished(&self) -> bool {
        self.phase == ExecutorPhase::Finished
    }

    /// Returns true if there are still Pending items in the queue.
    fn has_remaining(&self) -> bool {
        self.items
            .iter()
            .any(|i| i.state == QueueItemState::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::types::GhIssue;
    use crate::work::dependencies::DependencyGraph;
    use crate::work::types::WorkItem;

    fn make_item(number: u64) -> WorkItem {
        WorkItem::from_issue(GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: vec![],
            state: "open".to_string(),
            html_url: String::new(),
            milestone: None,
            assignees: vec![],
        })
    }

    fn make_queue(issues: &[u64]) -> WorkQueue {
        let items: Vec<WorkItem> = issues.iter().map(|&n| make_item(n)).collect();
        let graph = DependencyGraph::build(&items);
        match WorkQueue::validate_selection(issues, &graph) {
            Ok(queue) => queue,
            Err(e) => panic!("test queue should validate: {e}"),
        }
    }

    fn advance_ok(exec: &mut QueueExecutor) -> Option<&QueuedItem> {
        match exec.advance() {
            Ok(item) => item,
            Err(e) => panic!("advance should succeed: {e}"),
        }
    }

    #[test]
    fn new_with_empty_queue_starts_idle() {
        let queue = make_queue(&[]);
        let exec = QueueExecutor::new(&queue);
        assert_eq!(exec.phase(), ExecutorPhase::Idle);
    }

    #[test]
    fn new_populates_items_as_pending() {
        let queue = make_queue(&[1, 2, 3]);
        let exec = QueueExecutor::new(&queue);
        assert!(
            exec.items()
                .iter()
                .all(|i| i.state == QueueItemState::Pending)
        );
    }

    #[test]
    fn total_count_equals_queue_length() {
        let queue = make_queue(&[10, 20, 30]);
        let exec = QueueExecutor::new(&queue);
        assert_eq!(exec.total_count(), 3);
    }

    #[test]
    fn completed_count_is_zero_on_new_executor() {
        let queue = make_queue(&[1, 2]);
        let exec = QueueExecutor::new(&queue);
        assert_eq!(exec.completed_count(), 0);
    }

    #[test]
    fn advance_from_idle_moves_to_running_index_zero() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        assert_eq!(exec.phase(), ExecutorPhase::Running { current_index: 0 });
    }

    #[test]
    fn advance_sets_current_item_state_to_running() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        assert_eq!(exec.items()[0].state, QueueItemState::Running);
    }

    #[test]
    fn set_session_id_attaches_uuid_to_current_item() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        let id = uuid::Uuid::new_v4();
        exec.set_session_id(id);
        assert_eq!(exec.items()[0].session_id, Some(id));
    }

    #[test]
    fn mark_success_on_last_item_moves_to_finished() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_success();
        assert_eq!(exec.phase(), ExecutorPhase::Finished);
    }

    #[test]
    fn mark_success_on_non_last_item_moves_to_idle() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_success();
        assert_eq!(exec.phase(), ExecutorPhase::Idle);
    }

    #[test]
    fn mark_success_sets_item_state_to_succeeded() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_success();
        assert_eq!(exec.items()[0].state, QueueItemState::Succeeded);
    }

    #[test]
    fn completed_count_increments_after_mark_success() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_success();
        assert_eq!(exec.completed_count(), 1);
    }

    #[test]
    fn mark_failure_moves_to_awaiting_decision() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        assert_eq!(
            exec.phase(),
            ExecutorPhase::AwaitingDecision { failed_index: 0 }
        );
    }

    #[test]
    fn mark_failure_sets_item_state_to_failed() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        assert_eq!(exec.items()[0].state, QueueItemState::Failed);
    }

    #[test]
    fn apply_decision_retry_moves_to_idle() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Retry);
        assert_eq!(exec.phase(), ExecutorPhase::Idle);
    }

    #[test]
    fn apply_decision_retry_resets_item_state_to_pending() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Retry);
        assert_eq!(exec.items()[0].state, QueueItemState::Pending);
    }

    #[test]
    fn apply_decision_skip_sets_item_state_to_skipped() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Skip);
        assert_eq!(exec.items()[0].state, QueueItemState::Skipped);
    }

    #[test]
    fn apply_decision_skip_on_last_item_moves_to_finished() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Skip);
        assert_eq!(exec.phase(), ExecutorPhase::Finished);
    }

    #[test]
    fn apply_decision_skip_on_non_last_item_moves_to_idle() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Skip);
        assert_eq!(exec.phase(), ExecutorPhase::Idle);
    }

    #[test]
    fn apply_decision_abort_always_moves_to_finished() {
        let queue = make_queue(&[1, 2, 3]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Abort);
        assert_eq!(exec.phase(), ExecutorPhase::Finished);
    }

    #[test]
    fn is_finished_returns_false_when_idle() {
        let queue = make_queue(&[1]);
        let exec = QueueExecutor::new(&queue);
        assert!(!exec.is_finished());
    }

    #[test]
    fn is_finished_returns_false_when_running() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        assert!(!exec.is_finished());
    }

    #[test]
    fn is_finished_returns_false_when_awaiting_decision() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_failure();
        assert!(!exec.is_finished());
    }

    #[test]
    fn is_finished_returns_true_when_finished() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);
        exec.mark_success();
        assert!(exec.is_finished());
    }

    #[test]
    fn full_happy_path_two_items_both_succeed() {
        let queue = make_queue(&[1, 2]);
        let mut exec = QueueExecutor::new(&queue);

        advance_ok(&mut exec);
        exec.mark_success();
        assert_eq!(exec.completed_count(), 1);

        advance_ok(&mut exec);
        exec.mark_success();
        assert_eq!(exec.completed_count(), 2);
        assert!(exec.is_finished());
    }

    #[test]
    fn full_path_retry_then_succeed() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);

        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Retry);

        advance_ok(&mut exec);
        exec.mark_success();
        assert!(exec.is_finished());
        assert_eq!(exec.completed_count(), 1);
    }

    #[test]
    fn full_path_abort_stops_mid_queue() {
        let queue = make_queue(&[1, 2, 3]);
        let mut exec = QueueExecutor::new(&queue);

        advance_ok(&mut exec);
        exec.mark_failure();
        exec.apply_decision(FailureAction::Abort);

        assert!(exec.is_finished());
        // Items 2 and 3 are still Pending
        assert_eq!(exec.items()[1].state, QueueItemState::Pending);
        assert_eq!(exec.items()[2].state, QueueItemState::Pending);
    }

    #[test]
    fn items_accessor_returns_all_executor_items() {
        let queue = make_queue(&[10, 20]);
        let exec = QueueExecutor::new(&queue);
        assert_eq!(exec.items().len(), exec.total_count());
    }

    #[test]
    fn phase_accessor_returns_current_phase() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        assert_eq!(exec.phase(), ExecutorPhase::Idle);
        advance_ok(&mut exec);
        assert!(matches!(exec.phase(), ExecutorPhase::Running { .. }));
    }

    #[test]
    fn advance_on_empty_queue_moves_directly_to_finished() {
        let queue = make_queue(&[]);
        let mut exec = QueueExecutor::new(&queue);
        let result = advance_ok(&mut exec);
        assert!(result.is_none());
        assert_eq!(exec.phase(), ExecutorPhase::Finished);
    }

    #[test]
    fn advance_while_running_returns_error() {
        let queue = make_queue(&[1]);
        let mut exec = QueueExecutor::new(&queue);
        advance_ok(&mut exec);

        let err = match exec.advance() {
            Ok(_) => panic!("second advance while running should fail"),
            Err(e) => e,
        };

        assert!(err.to_string().contains("cannot advance queue"));
    }
}
