use chrono::Utc;

use super::intent::SessionIntent;
use super::types::{Session, SessionStatus};
use crate::config::{HollowRetryConfig, HollowRetryPolicy};

/// Retry policy configuration.
pub struct RetryPolicy {
    pub max_retries: u32,
    pub cooldown_secs: u64,
    pub hollow: HollowRetryConfig,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, cooldown_secs: u64, hollow: HollowRetryConfig) -> Self {
        Self {
            max_retries,
            cooldown_secs,
            hollow,
        }
    }

    pub fn from_config(cfg: &crate::config::SessionsConfig) -> Self {
        Self::new(
            cfg.max_retries,
            cfg.retry_cooldown_secs,
            cfg.hollow_retry.clone(),
        )
    }

    /// Retry budget for `session`. Non-hollow sessions always get
    /// `self.max_retries`; hollow sessions route through the configured
    /// `HollowRetryPolicy`. The `Always` arm deliberately uses
    /// `work_max_retries` for both intents — its contract is "one knob
    /// governs every hollow session".
    pub fn effective_max(&self, session: &Session) -> u32 {
        if !session.is_hollow_completion {
            return self.max_retries;
        }
        match self.hollow.policy {
            HollowRetryPolicy::Never => 0,
            HollowRetryPolicy::Always => self.hollow.work_max_retries,
            HollowRetryPolicy::IntentAware => match session.intent {
                SessionIntent::Work => self.hollow.work_max_retries,
                SessionIntent::Consultation => self.hollow.consultation_max_retries,
            },
        }
    }

    /// Returns true when a hollow-completed session is actually a consultation
    /// prompt (e.g. "how are you?") that produced a text response. The session
    /// "did its job" — retrying it would just spawn a duplicate answer.
    pub fn is_consultation_satisfied(session: &Session) -> bool {
        session.is_hollow_completion
            && session.intent == SessionIntent::Consultation
            && !session.last_message.trim().is_empty()
    }

    /// Check if a session is eligible for retry based on its status and retry count.
    pub fn should_retry(&self, session: &Session) -> bool {
        if Self::is_consultation_satisfied(session) {
            return false;
        }

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

    /// Build a `HollowRetryConfig` matching the pre-#275 flat
    /// `hollow_max_retries = N` semantics: `IntentAware` policy,
    /// `work_max_retries = N`, `consultation_max_retries = 0`. Used by
    /// legacy tests to preserve their original assertions.
    fn legacy_hollow(work_max_retries: u32) -> HollowRetryConfig {
        HollowRetryConfig {
            policy: HollowRetryPolicy::IntentAware,
            work_max_retries,
            consultation_max_retries: 0,
        }
    }

    fn policy_with(policy: HollowRetryPolicy, work: u32, consultation: u32) -> RetryPolicy {
        RetryPolicy::new(
            5,
            0,
            HollowRetryConfig {
                policy,
                work_max_retries: work,
                consultation_max_retries: consultation,
            },
        )
    }

    fn hollow_session_with_intent(intent: SessionIntent) -> Session {
        let mut s = Session::new("prompt".into(), "opus".into(), "orchestrator".into(), None);
        s.status = SessionStatus::Completed;
        s.is_hollow_completion = true;
        s.intent = intent;
        s
    }

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
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Stalled, 0);
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_true_for_errored_under_max() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Errored, 0);
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_when_max_reached() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Stalled, 2);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_completed() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Completed, 0);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_running() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Running, 0);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_killed() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Killed, 0);
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_respects_cooldown() {
        let policy = RetryPolicy::new(2, 9999, legacy_hollow(1));
        let mut session = make_session(SessionStatus::Stalled, 0);
        session.last_retry_at = Some(Utc::now());
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_allows_after_cooldown() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let mut session = make_session(SessionStatus::Stalled, 0);
        session.last_retry_at = Some(Utc::now() - chrono::Duration::seconds(100));
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn prepare_retry_increments_count() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.retry_count, 1);
    }

    #[test]
    fn prepare_retry_sets_last_retry_at() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.last_retry_at.is_some());
    }

    #[test]
    fn prepare_retry_preserves_issue_number() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Errored, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.issue_number, Some(1));
    }

    #[test]
    fn prepare_retry_appends_context_to_prompt() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.prompt.contains("RETRY CONTEXT"));
        assert!(retry.prompt.contains("attempt 1 of 2"));
        assert!(retry.prompt.contains("test prompt"));
    }

    #[test]
    fn prepare_retry_resets_status_to_queued() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Errored, 1);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.status, SessionStatus::Queued);
    }

    #[test]
    fn prepare_retry_preserves_model_and_mode() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert_eq!(retry.model, "opus");
        assert_eq!(retry.mode, "orchestrator");
    }

    #[test]
    fn prepare_retry_includes_progress_context() {
        use crate::state::progress::{SessionPhase, SessionProgress};
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
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
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let original = make_session(SessionStatus::Stalled, 0);
        let retry = policy.prepare_retry(&original, None, None);
        assert!(!retry.prompt.contains("Phase:"));
        assert!(!retry.prompt.contains("Tools used:"));
    }

    #[test]
    fn zero_max_retries_never_retries() {
        let policy = RetryPolicy::new(0, 0, legacy_hollow(1));
        let session = make_session(SessionStatus::Stalled, 0);
        assert!(!policy.should_retry(&session));
    }

    // --- Issue #171: Hollow completion retry tests ---

    #[test]
    fn should_retry_true_for_hollow_completion_under_max() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let mut session = make_session(SessionStatus::Completed, 0);
        session.is_hollow_completion = true;
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_hollow_completion_at_max() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(1));
        let mut session = make_session(SessionStatus::Completed, 1);
        session.is_hollow_completion = true;
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_hollow_when_hollow_max_is_zero() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(0));
        let mut session = make_session(SessionStatus::Completed, 0);
        session.is_hollow_completion = true;
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_hollow_respects_cooldown() {
        let policy = RetryPolicy::new(2, 9999, legacy_hollow(1));
        let mut session = make_session(SessionStatus::Completed, 0);
        session.is_hollow_completion = true;
        session.last_retry_at = Some(Utc::now());
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn prepare_retry_hollow_includes_hollow_context() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(2));
        let mut original = make_session(SessionStatus::Completed, 0);
        original.is_hollow_completion = true;
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.prompt.contains("hollow completion"));
        assert!(retry.prompt.contains("re-read the task"));
    }

    // --- Issue #274: Skip hollow retry for consultation/Q&A prompts ---

    fn make_hollow_consultation(response: &str) -> Session {
        let mut s = Session::new(
            "how are you?".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
        );
        s.status = SessionStatus::Completed;
        s.is_hollow_completion = true;
        s.last_message = response.to_string();
        s
    }

    fn make_hollow_work() -> Session {
        let mut s = Session::new(
            "fix the bug in login".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
        );
        s.status = SessionStatus::Completed;
        s.is_hollow_completion = true;
        s
    }

    #[test]
    fn should_retry_false_for_hollow_consultation_with_response() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(2));
        let session = make_hollow_consultation("I'm doing well, thanks!");
        assert!(
            !policy.should_retry(&session),
            "consultation prompt with a response should not retry"
        );
    }

    #[test]
    fn should_retry_true_for_hollow_work_session() {
        let policy = RetryPolicy::new(2, 0, legacy_hollow(2));
        let session = make_hollow_work();
        assert!(
            policy.should_retry(&session),
            "work session that went hollow must still retry"
        );
    }

    #[test]
    fn should_retry_true_for_hollow_consultation_with_empty_response() {
        // If the consultation produced no response, something went wrong —
        // we still want to retry it, PROVIDED the configured policy allows
        // consultation retries. Under the new #275 defaults
        // (consultation_max_retries=0), this case doesn't retry — that's
        // covered in a separate #275 group test. Here we exercise the
        // long-standing "empty response means not-yet-satisfied" invariant
        // using a policy with a non-zero consultation limit.
        let policy = policy_with(HollowRetryPolicy::IntentAware, 2, 2);
        let session = make_hollow_consultation("");
        assert!(
            policy.should_retry(&session),
            "empty consultation response should still be retried when consultation_max > 0"
        );
    }

    #[test]
    fn should_retry_true_for_hollow_consultation_whitespace_only_response() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 2, 2);
        let session = make_hollow_consultation("   \n\t  ");
        assert!(
            policy.should_retry(&session),
            "whitespace-only response counts as empty"
        );
    }

    #[test]
    fn is_consultation_satisfied_true_when_all_conditions_met() {
        let session = make_hollow_consultation("some answer");
        assert!(RetryPolicy::is_consultation_satisfied(&session));
    }

    #[test]
    fn is_consultation_satisfied_false_for_work_intent() {
        let session = make_hollow_work();
        assert!(!RetryPolicy::is_consultation_satisfied(&session));
    }

    #[test]
    fn is_consultation_satisfied_false_when_not_hollow() {
        let mut session = make_hollow_consultation("answer");
        session.is_hollow_completion = false;
        assert!(!RetryPolicy::is_consultation_satisfied(&session));
    }

    #[test]
    fn is_consultation_satisfied_false_with_empty_response() {
        let session = make_hollow_consultation("");
        assert!(!RetryPolicy::is_consultation_satisfied(&session));
    }

    #[test]
    fn prepare_retry_hollow_shows_correct_max() {
        let policy = RetryPolicy::new(5, 0, legacy_hollow(2));
        let mut original = make_session(SessionStatus::Completed, 0);
        original.is_hollow_completion = true;
        let retry = policy.prepare_retry(&original, None, None);
        assert!(retry.prompt.contains("attempt 1 of 2"));
    }

    // --- Issue #275: configurable hollow retry policy ---
    // Group E: effective_max matrix across {policy} × {intent} × {is_hollow}.

    #[test]
    fn effective_max_non_hollow_returns_max_retries() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 2, 0);
        let session = make_session(SessionStatus::Errored, 0);
        assert!(!session.is_hollow_completion);
        assert_eq!(policy.effective_max(&session), 5);
    }

    #[test]
    fn effective_max_never_policy_returns_zero_for_work_hollow() {
        let policy = policy_with(HollowRetryPolicy::Never, 3, 0);
        let session = hollow_session_with_intent(SessionIntent::Work);
        assert_eq!(policy.effective_max(&session), 0);
    }

    #[test]
    fn effective_max_never_policy_returns_zero_for_consultation_hollow() {
        let policy = policy_with(HollowRetryPolicy::Never, 3, 0);
        let session = hollow_session_with_intent(SessionIntent::Consultation);
        assert_eq!(policy.effective_max(&session), 0);
    }

    #[test]
    fn effective_max_always_policy_returns_work_for_work_hollow() {
        let policy = policy_with(HollowRetryPolicy::Always, 4, 1);
        let session = hollow_session_with_intent(SessionIntent::Work);
        assert_eq!(policy.effective_max(&session), 4);
    }

    #[test]
    fn effective_max_always_policy_returns_work_for_consultation_hollow() {
        // Always policy documents "one knob governs every hollow session":
        // consultation sessions use work_max_retries, not consultation_max_retries.
        let policy = policy_with(HollowRetryPolicy::Always, 4, 1);
        let session = hollow_session_with_intent(SessionIntent::Consultation);
        assert_eq!(policy.effective_max(&session), 4);
    }

    #[test]
    fn effective_max_intent_aware_work_returns_work_limit() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 3, 0);
        let session = hollow_session_with_intent(SessionIntent::Work);
        assert_eq!(policy.effective_max(&session), 3);
    }

    #[test]
    fn effective_max_intent_aware_consultation_returns_consultation_limit() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 3, 1);
        let session = hollow_session_with_intent(SessionIntent::Consultation);
        assert_eq!(policy.effective_max(&session), 1);
    }

    // Group F: should_retry regression with new config shape.

    #[test]
    fn should_retry_false_for_consultation_when_consultation_max_is_zero() {
        // Empty last_message so is_consultation_satisfied is false — the
        // retry-count-vs-max check must carry the weight.
        let policy = policy_with(HollowRetryPolicy::IntentAware, 2, 0);
        let mut session = hollow_session_with_intent(SessionIntent::Consultation);
        session.last_message = String::new();
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_true_for_work_hollow_under_work_max() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 2, 0);
        let mut session = hollow_session_with_intent(SessionIntent::Work);
        session.retry_count = 0;
        assert!(policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_work_hollow_at_work_max() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 2, 0);
        let mut session = hollow_session_with_intent(SessionIntent::Work);
        session.retry_count = 2;
        assert!(!policy.should_retry(&session));
    }

    #[test]
    fn should_retry_false_for_never_policy_any_intent() {
        let policy = policy_with(HollowRetryPolicy::Never, 3, 1);
        let session = hollow_session_with_intent(SessionIntent::Work);
        assert!(!policy.should_retry(&session));
        let session = hollow_session_with_intent(SessionIntent::Consultation);
        assert!(!policy.should_retry(&session));
    }

    // Group G: from_config integration.

    #[test]
    fn from_config_reads_hollow_retry_struct() {
        let toml_str = r#"
[hollow_retry]
policy = "never"
work_max_retries = 9
consultation_max_retries = 3
"#;
        let cfg: crate::config::SessionsConfig = toml::from_str(toml_str).expect("parse failed");
        let policy = RetryPolicy::from_config(&cfg);
        let session = hollow_session_with_intent(SessionIntent::Work);
        assert_eq!(policy.effective_max(&session), 0);
    }

    // Group I: completion_pipeline contract — effective_max is the
    // canonical source of the HollowRetryScreen's max, replacing the
    // removed `p.hollow_max_retries` direct access.

    #[test]
    fn hollow_retry_screen_max_reflects_effective_max_not_work_limit() {
        let policy = policy_with(HollowRetryPolicy::IntentAware, 3, 0);
        let mut session = hollow_session_with_intent(SessionIntent::Consultation);
        session.last_message = String::new();
        session.retry_count = 0;
        // A consultation hollow session with consultation_max=0 must report
        // effective_max=0, NOT the work_max_retries of 3.
        assert_eq!(policy.effective_max(&session), 0);
    }
}
