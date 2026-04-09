use chrono::Utc;

use super::types::{Session, SessionStatus};

/// Retry policy configuration.
pub struct RetryPolicy {
    pub max_retries: u32,
    pub cooldown_secs: u64,
    pub hollow_max_retries: u32,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, cooldown_secs: u64, hollow_max_retries: u32) -> Self {
        Self {
            max_retries,
            cooldown_secs,
            hollow_max_retries,
        }
    }

    pub fn from_config(cfg: &crate::config::SessionsConfig) -> Self {
        Self::new(
            cfg.max_retries,
            cfg.retry_cooldown_secs,
            cfg.hollow_max_retries,
        )
    }

    /// The effective max retries for a session, accounting for hollow completions.
    pub fn effective_max(&self, session: &Session) -> u32 {
        if session.is_hollow_completion {
            self.hollow_max_retries
        } else {
            self.max_retries
        }
    }

    /// Check if a session is eligible for retry based on its status and retry count.
    pub fn should_retry(&self, session: &Session) -> bool {
        let max = self.effective_max(session);

        if session.retry_count >= max {
            return false;
        }

        let eligible = matches!(
            session.status,
            SessionStatus::Stalled | SessionStatus::Errored
        ) || (session.status == SessionStatus::Completed
            && session.is_hollow_completion);

        if !eligible {
            return false;
        }

        // Check cooldown
        if let Some(last_retry) = session.last_retry_at {
            let elapsed = (Utc::now() - last_retry).num_seconds();
            if elapsed < self.cooldown_secs as i64 {
                return false;
            }
        }

        true
    }

    /// Create a new session for retrying a failed/stalled one.
    /// Increments retry count and appends rich context to the prompt.
    pub fn prepare_retry(
        &self,
        original: &Session,
        progress: Option<&crate::state::progress::SessionProgress>,
        last_error: Option<&str>,
    ) -> Session {
        let status_desc = if original.is_hollow_completion {
            "completed without performing any work (hollow completion)"
        } else if original.status == SessionStatus::Stalled {
            "stalled (no output produced)"
        } else {
            "failed with an error"
        };

        let max = self.effective_max(original);

        let mut retry_context = format!(
            "\n\n--- RETRY CONTEXT (attempt {} of {}) ---\n\
             Previous attempt {} after status: {}.",
            original.retry_count + 1,
            max,
            status_desc,
            original.status.label()
        );

        // Append progress details if available
        if let Some(prog) = progress {
            retry_context.push_str(&format!(
                "\nPrevious attempt reached phase: {}",
                prog.phase.label()
            ));
            if !prog.files_at_checkpoint.is_empty() {
                retry_context.push_str(&format!(
                    "\nFiles modified: {}",
                    prog.files_at_checkpoint.join(", ")
                ));
            }
            retry_context.push_str(&format!("\nTools used: {}", prog.tools_used_count));
        }

        // Append last error if available
        if let Some(err) = last_error {
            retry_context.push_str(&format!("\nLast error: {}", err));
        }

        if original.is_hollow_completion {
            retry_context.push_str(
                "\nThe previous attempt completed without doing any work. \
                 Please re-read the task and execute it fully this time.",
            );
        } else {
            retry_context.push_str("\nPlease review the existing changes and fix the issues.");
        }

        let mut new_session = Session::new(
            format!("{}{}", original.prompt, retry_context),
            original.model.clone(),
            original.mode.clone(),
            original.issue_number,
        );
        new_session.retry_count = original.retry_count + 1;
        new_session.last_retry_at = Some(Utc::now());
        new_session.issue_title = original.issue_title.clone();
        new_session
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(status: SessionStatus, retry_count: u32) -> Session {
        let mut s = Session::new(
            "test prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
        );
        s.status = status;
        s.retry_count = retry_count;
        s
    }

    #[test]
    fn should_retry_true_for_stalled_under_max() {
        let policy = RetryPolicy::new(2, 0, 1);
        let session = make_session(SessionStatus::Stalled, 0);
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_true_for_errored_under_max() {
        let policy = RetryPolicy::new(2, 0, 1);
        let session = make_session(SessionStatus::Errored, 0);
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_when_max_reached() {
        let policy = RetryPolicy::new(2, 0, 1);
        let session = make_session(SessionStatus::Stalled, 2);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_completed() {
        let policy = RetryPolicy::new(2, 0, 1);
        let session = make_session(SessionStatus::Completed, 0);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_running() {
        let policy = RetryPolicy::new(2, 0, 1);
        let session = make_session(SessionStatus::Running, 0);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_killed() {
        let policy = RetryPolicy::new(2, 0, 1);
        let session = make_session(SessionStatus::Killed, 0);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_respects_cooldown() {
        let policy = RetryPolicy::new(2, 9999, 1);
        let mut session = make_session(SessionStatus::Stalled, 0);
        session.last_retry_at = Some(Utc::now());
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_allows_after_cooldown() {
        let policy = RetryPolicy::new(2, 0, 1);
        let mut session = make_session(SessionStatus::Stalled, 0);
        session.last_retry_at = Some(Utc::now() - chrono::Duration::seconds(100));
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn prepare_retry_increments_count() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.retry_count, 1);
    }

    #[test]
    fn prepare_retry_sets_last_retry_at() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.last_retry_at.is_some());
    }

    #[test]
    fn prepare_retry_preserves_issue_number() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Errored, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.issue_number, Some(1));
    }

    #[test]
    fn prepare_retry_appends_context_to_prompt() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.prompt.contains("RETRY CONTEXT"));
        assert!(retry.prompt.contains("attempt 1 of 2"));
        assert!(retry.prompt.contains("test prompt"));
    }

    #[test]
    fn prepare_retry_resets_status_to_queued() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Errored, 1);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.status, SessionStatus::Queued);
    }

    #[test]
    fn prepare_retry_preserves_model_and_mode() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.model, "opus");
        assert_eq!(retry.mode, "orchestrator");
    }

    #[test]
    fn prepare_retry_includes_progress_context() {
        use crate::state::progress::{SessionPhase, SessionProgress};
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Errored, 0);
        let mut progress = SessionProgress::new();
        progress.phase = SessionPhase::Implementing;
        progress.tools_used_count = 47;
        progress.files_at_checkpoint = vec!["src/foo.rs".into(), "src/bar.rs".into()];
        let retry = policy.prepare_retry(
            &original,
            Some(&progress),
            Some("tests failed with 3 failures"),
        );
        assert!(retry.prompt.contains("IMPLEMENTING"));
        assert!(retry.prompt.contains("src/foo.rs, src/bar.rs"));
        assert!(retry.prompt.contains("Tools used: 47"));
        assert!(retry.prompt.contains("tests failed with 3 failures"));
    }

    #[test]
    fn prepare_retry_without_progress_omits_details() {
        let policy = RetryPolicy::new(2, 0, 1);
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert!(!retry.prompt.contains("Phase:"));
        assert!(!retry.prompt.contains("Tools used:"));
    }

    #[test]
    fn zero_max_retries_never_retries() {
        let policy = RetryPolicy::new(0, 0, 1);
        let session = make_session(SessionStatus::Stalled, 0);
        assert!(!policy.should_retry(&session));
    }

    // --- Issue #171: Hollow completion retry tests ---

    #[test]
    fn should_retry_true_for_hollow_completion_under_max() {
        let policy = RetryPolicy::new(2, 0, 1);
        let mut session = make_session(SessionStatus::Completed, 0);
        session.is_hollow_completion = true;
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_hollow_completion_at_max() {
        let policy = RetryPolicy::new(2, 0, 1);
        let mut session = make_session(SessionStatus::Completed, 1);
        session.is_hollow_completion = true;
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_hollow_when_hollow_max_is_zero() {
        let policy = RetryPolicy::new(2, 0, 0);
        let mut session = make_session(SessionStatus::Completed, 0);
        session.is_hollow_completion = true;
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_hollow_respects_cooldown() {
        let policy = RetryPolicy::new(2, 9999, 1);
        let mut session = make_session(SessionStatus::Completed, 0);
        session.is_hollow_completion = true;
        session.last_retry_at = Some(Utc::now());
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn prepare_retry_hollow_includes_hollow_context() {
        let policy = RetryPolicy::new(2, 0, 2);
        let mut original = make_session(SessionStatus::Completed, 0);
        original.is_hollow_completion = true;
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.prompt.contains("hollow completion"));
        assert!(retry.prompt.contains("re-read the task"));
    }

    #[test]
    fn prepare_retry_hollow_shows_correct_max() {
        let policy = RetryPolicy::new(5, 0, 2);
        let mut original = make_session(SessionStatus::Completed, 0);
        original.is_hollow_completion = true;
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.prompt.contains("attempt 1 of 2"));
    }
}
