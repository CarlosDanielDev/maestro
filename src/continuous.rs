/// State machine for continuous work mode (`--continuous` / `-C` flag).
///
/// In continuous mode, maestro auto-advances to the next ready issue after each
/// session completion. When a session fails, the loop pauses until the user
/// presses `s` (skip), `r` (retry), or `q` (quit).
#[derive(Debug, Clone)]
pub struct ContinuousModeState {
    /// Whether the loop is currently paused waiting for user decision.
    paused: bool,
    /// Accumulated failure records across the entire run.
    pub failures: Vec<ContinuousFailure>,
    /// Count of issues completed successfully this run.
    pub completed_count: usize,
    /// Count of issues skipped this run.
    pub skipped_count: usize,
    /// Issue currently being worked on (None when between issues).
    pub current_issue: Option<u64>,
}

/// A single issue failure recorded during a continuous run.
#[derive(Debug, Clone)]
pub struct ContinuousFailure {
    pub issue_number: u64,
    pub issue_title: String,
    pub error_summary: String,
}

impl ContinuousModeState {
    pub fn new() -> Self {
        Self {
            paused: false,
            failures: Vec::new(),
            completed_count: 0,
            skipped_count: 0,
            current_issue: None,
        }
    }

    /// Called when an issue session completes successfully.
    pub fn on_issue_completed(&mut self, _issue_number: u64) {
        self.completed_count += 1;
        self.current_issue = None;
    }

    /// Called when an issue session fails. Pauses the loop.
    pub fn on_issue_failed(
        &mut self,
        issue_number: u64,
        issue_title: String,
        error_summary: String,
    ) {
        self.paused = true;
        self.current_issue = None;
        self.failures.push(ContinuousFailure {
            issue_number,
            issue_title,
            error_summary,
        });
    }

    /// User pressed `s` — skip the failed issue, resume advancing.
    pub fn on_skip(&mut self) {
        self.paused = false;
        self.skipped_count += 1;
    }

    /// User pressed `r` — retry the failed issue, resume advancing.
    /// Returns the issue number to retry, if any.
    pub fn on_retry(&mut self) -> Option<u64> {
        self.paused = false;
        self.failures.last().map(|f| f.issue_number)
    }

    /// Returns true when the loop may advance to the next ready issue.
    pub fn can_advance(&self) -> bool {
        !self.paused && self.current_issue.is_none()
    }

    /// Returns the current failure the user needs to decide on, if paused.
    pub fn current_failure(&self) -> Option<&ContinuousFailure> {
        if self.paused {
            self.failures.last()
        } else {
            None
        }
    }

    /// Total number of issues attempted so far (completed + skipped + failed).
    pub fn total_attempted(&self) -> usize {
        self.completed_count + self.skipped_count + self.failures.len()
    }

    /// Track that an issue has been launched.
    pub fn set_current_issue(&mut self, issue_number: u64) {
        self.current_issue = Some(issue_number);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_active_with_no_failures() {
        let state = ContinuousModeState::new();
        assert!(state.can_advance());
        assert!(state.failures.is_empty());
    }

    #[test]
    fn on_issue_completed_increments_count_and_allows_advance() {
        let mut state = ContinuousModeState::new();
        state.set_current_issue(1);
        state.on_issue_completed(1);
        assert!(state.can_advance());
        assert_eq!(state.completed_count, 1);
        assert!(state.current_issue.is_none());
    }

    #[test]
    fn on_issue_failed_sets_paused_and_records_failure() {
        let mut state = ContinuousModeState::new();
        state.set_current_issue(42);
        state.on_issue_failed(42, "Fix login".into(), "Compilation error".into());
        assert!(!state.can_advance());
        assert_eq!(state.failures.len(), 1);
        assert_eq!(state.failures[0].issue_number, 42);
        assert!(state.current_issue.is_none());
    }

    #[test]
    fn on_skip_resumes_advancing() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(1, "Title".into(), "Error".into());
        assert!(!state.can_advance());
        state.on_skip();
        assert!(state.can_advance());
        assert_eq!(state.skipped_count, 1);
    }

    #[test]
    fn on_retry_resumes_advancing_and_returns_issue_number() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(1, "Title".into(), "Error".into());
        assert!(!state.can_advance());
        let retry_num = state.on_retry();
        assert!(state.can_advance());
        assert_eq!(retry_num, Some(1));
    }

    #[test]
    fn on_issue_failed_accumulates_multiple_failures() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(1, "First".into(), "Err1".into());
        state.on_skip();
        state.on_issue_failed(2, "Second".into(), "Err2".into());
        assert_eq!(state.failures.len(), 2);
    }

    #[test]
    fn failures_retain_title_and_error_summary() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(7, "Add auth".into(), "Timeout error".into());
        let f = &state.failures[0];
        assert_eq!(f.issue_number, 7);
        assert_eq!(f.issue_title, "Add auth");
        assert_eq!(f.error_summary, "Timeout error");
    }

    #[test]
    fn on_skip_does_not_clear_failure_history() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(1, "Title".into(), "Error".into());
        state.on_skip();
        assert_eq!(state.failures.len(), 1);
        assert!(state.can_advance());
    }

    #[test]
    fn on_retry_does_not_clear_failure_history() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(1, "Title".into(), "Error".into());
        state.on_retry();
        assert_eq!(state.failures.len(), 1);
    }

    #[test]
    fn can_advance_false_when_issue_running() {
        let mut state = ContinuousModeState::new();
        state.set_current_issue(5);
        assert!(!state.can_advance());
    }

    #[test]
    fn current_failure_returns_none_when_not_paused() {
        let state = ContinuousModeState::new();
        assert!(state.current_failure().is_none());
    }

    #[test]
    fn current_failure_returns_failure_when_paused() {
        let mut state = ContinuousModeState::new();
        state.on_issue_failed(42, "Title".into(), "Error".into());
        let f = state.current_failure().unwrap();
        assert_eq!(f.issue_number, 42);
    }
}
