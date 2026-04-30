use super::App;
use crate::provider::github::ci::PendingPrCheck;
use crate::provider::github::pr::{PrCreator, PrRetryPolicy};
use crate::provider::github::types::{
    PENDING_PR_LAST_ERRORS_CAP, PENDING_PR_MANUAL_RETRY_LIFETIME_CAP, PendingPrStatus,
};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;
use std::time::Instant;

impl App {
    pub async fn process_pending_pr_retries(&mut self) {
        if self.pending_prs.is_empty() || self.github_client.is_none() {
            return;
        }

        let now = chrono::Utc::now();

        // Collect indices of items ready for retry (avoids borrow issues with self)
        let ready_indices: Vec<usize> = self
            .pending_prs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                if p.status == PendingPrStatus::RetryScheduled {
                    p.next_retry_at.is_none_or(|next| now >= next)
                } else {
                    p.status == PendingPrStatus::Retrying
                }
            })
            .map(|(i, _)| i)
            .collect();

        let mut completed_indices = Vec::new();
        let policy = PrRetryPolicy::default();

        for idx in ready_indices {
            let pending = &mut self.pending_prs[idx];
            pending.attempt += 1;
            pending.status = PendingPrStatus::Retrying;
            pending.last_attempt_at = now;

            let issue_number = pending.issue_number;
            let branch = pending.branch.clone();
            let base_branch = pending.base_branch.clone();
            let files_touched: Vec<String> = pending.files_touched.clone();
            let cost_usd = pending.cost_usd;
            let attempt = pending.attempt;

            let issue = self.state.issue_cache.get(&issue_number).cloned();
            if let (Some(client), Some(issue)) = (&self.github_client, &issue) {
                let file_refs: Vec<&str> = files_touched.iter().map(|s| s.as_str()).collect();
                let pr_creator = PrCreator::new(client.as_ref(), base_branch);
                match pr_creator
                    .create_for_issue(issue, &branch, &file_refs, cost_usd)
                    .await
                {
                    Ok(pr_num) => {
                        completed_indices.push(idx);
                        if let Some(managed) = self.pool.find_by_issue_mut(issue_number) {
                            let _ = managed.session.transition_to(
                                SessionStatus::Completed,
                                TransitionReason::StreamCompleted,
                            );
                        }
                        self.activity_log.push_simple(
                            format!("#{}", issue_number),
                            format!("PR #{} created (retry {})", pr_num, attempt),
                            LogLevel::Info,
                        );
                        self.ci_poller.add_check(PendingPrCheck {
                            pr_number: pr_num,
                            issue_number,
                            branch: branch.clone(),
                            created_at: Instant::now(),
                            check_count: 0,
                            fix_attempt: 0,
                            awaiting_fix_ci: false,
                        });
                    }
                    Err(e) => {
                        let err_str = e.to_string().trim_end().to_string();
                        let pending = &mut self.pending_prs[idx];
                        pending.last_error = err_str.clone();
                        record_error_for_correlation(pending, err_str.clone());

                        if errors_match_threshold(pending) {
                            pending.status = PendingPrStatus::PermanentlyFailed;
                            pending.next_retry_at = None;
                            self.activity_log.push_simple(
                                format!("#{}", issue_number),
                                format!(
                                    "PR retry stuck on identical error {}×. Branch: {}. \
                                     Stderr captured. Run `gh pr create --base {} --head {}` \
                                     manually or file a bug.",
                                    PENDING_PR_LAST_ERRORS_CAP, branch, pending.base_branch, branch,
                                ),
                                LogLevel::Error,
                            );
                        } else if let Some(delay) = policy.delay_for_attempt(attempt) {
                            pending.next_retry_at = Some(
                                now + chrono::Duration::from_std(delay)
                                    .unwrap_or(chrono::Duration::seconds(5)),
                            );
                            pending.status = PendingPrStatus::RetryScheduled;
                            self.activity_log.push_simple(
                                format!("#{}", issue_number),
                                format!(
                                    "PR retry {} failed, next in {}s: {}",
                                    attempt,
                                    delay.as_secs(),
                                    e
                                ),
                                LogLevel::Warn,
                            );
                        } else {
                            pending.status = PendingPrStatus::AwaitingManualRetry;
                            pending.next_retry_at = None;
                            self.activity_log.push_simple(
                                format!("#{}", issue_number),
                                format!(
                                    "PR creation failed permanently after {} attempts: {}",
                                    attempt, e
                                ),
                                LogLevel::Error,
                            );
                        }
                    }
                }
            }
        }

        // Remove completed entries (reverse order to preserve indices)
        for idx in completed_indices.into_iter().rev() {
            self.pending_prs.remove(idx);
        }
    }

    /// Trigger a manual PR retry for a specific issue. Called from TUI key handler.
    ///
    /// Skips entries already in `PermanentlyFailed`. Increments
    /// `manual_retry_count` and transitions to `PermanentlyFailed` if the
    /// lifetime cap is reached, so the user gets a clear "stop, file a bug"
    /// signal instead of looping forever on a deterministic failure.
    pub fn trigger_manual_pr_retry(&mut self, issue_number: u64) {
        let Some(pending) = self
            .pending_prs
            .iter_mut()
            .find(|p| p.issue_number == issue_number)
        else {
            return;
        };

        if pending.status == PendingPrStatus::PermanentlyFailed {
            return;
        }

        pending.manual_retry_count = pending.manual_retry_count.saturating_add(1);
        if pending.manual_retry_count > PENDING_PR_MANUAL_RETRY_LIFETIME_CAP {
            pending.status = PendingPrStatus::PermanentlyFailed;
            pending.next_retry_at = None;
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                format!(
                    "Manual PR retries exhausted (>{}). Branch: {}. Run \
                     `gh pr create --base {} --head {}` manually or file a bug.",
                    PENDING_PR_MANUAL_RETRY_LIFETIME_CAP,
                    pending.branch,
                    pending.base_branch,
                    pending.branch,
                ),
                LogLevel::Error,
            );
            return;
        }

        pending.status = PendingPrStatus::RetryScheduled;
        pending.next_retry_at = Some(chrono::Utc::now()); // immediate
        pending.attempt = 0; // reset attempt counter for manual retry
        self.activity_log.push_simple(
            format!("#{}", issue_number),
            "Manual PR retry queued".into(),
            LogLevel::Info,
        );
    }
}

/// Push `err` into `pending.last_errors`, evicting the oldest entry once the
/// cap is reached. Pure helper — separate fn so tests can drive it directly.
fn record_error_for_correlation(
    pending: &mut crate::provider::github::types::PendingPr,
    err: String,
) {
    while pending.last_errors.len() >= PENDING_PR_LAST_ERRORS_CAP {
        pending.last_errors.pop_front();
    }
    pending.last_errors.push_back(err);
}

/// True iff `pending.last_errors` contains exactly `PENDING_PR_LAST_ERRORS_CAP`
/// entries AND every entry is byte-equal — a deterministic-failure signal.
fn errors_match_threshold(pending: &crate::provider::github::types::PendingPr) -> bool {
    if pending.last_errors.len() < PENDING_PR_LAST_ERRORS_CAP {
        return false;
    }
    let mut iter = pending.last_errors.iter();
    let first = match iter.next() {
        Some(s) => s,
        None => return false,
    };
    iter.all(|s| s == first)
}

#[cfg(test)]
mod tests {
    use super::{errors_match_threshold, record_error_for_correlation};
    use crate::provider::github::types::{
        PENDING_PR_LAST_ERRORS_CAP, PENDING_PR_MANUAL_RETRY_LIFETIME_CAP, PendingPr,
        PendingPrStatus,
    };
    use crate::tui::activity_log::LogLevel;
    use std::collections::VecDeque;

    fn make_pending_pr(issue_number: u64) -> PendingPr {
        PendingPr {
            issue_number,
            issue_numbers: vec![],
            branch: format!("maestro/issue-{}", issue_number),
            base_branch: "main".into(),
            files_touched: vec![],
            cost_usd: 0.0,
            attempt: 3,
            max_attempts: 3,
            last_error: String::new(),
            last_attempt_at: chrono::Utc::now(),
            next_retry_at: None,
            status: PendingPrStatus::AwaitingManualRetry,
            last_errors: VecDeque::new(),
            manual_retry_count: 0,
        }
    }

    #[test]
    fn trigger_manual_pr_retry_matching_issue_resets_attempt_and_logs() {
        let mut app = crate::tui::make_test_app("pr-retry-match");
        app.pending_prs.push(make_pending_pr(42));

        app.trigger_manual_pr_retry(42);

        let p = &app.pending_prs[0];
        assert_eq!(p.status, PendingPrStatus::RetryScheduled);
        assert!(p.next_retry_at.is_some(), "next_retry_at must be set");
        assert_eq!(p.attempt, 0, "attempt counter must reset to 0");

        let last = app
            .activity_log
            .entries()
            .last()
            .expect("log must not be empty");
        assert_eq!(last.session_label, "#42");
        assert_eq!(last.level, LogLevel::Info);
        assert!(
            last.message.contains("Manual PR retry queued"),
            "got: {}",
            last.message
        );
    }

    #[test]
    fn trigger_manual_pr_retry_no_match_is_noop() {
        let mut app = crate::tui::make_test_app("pr-retry-noop");
        app.pending_prs.push(make_pending_pr(99));
        let log_len_before = app.activity_log.entries().len();

        app.trigger_manual_pr_retry(42);

        assert_eq!(
            app.pending_prs[0].status,
            PendingPrStatus::AwaitingManualRetry,
            "unrelated entry must be untouched"
        );
        assert_eq!(
            app.activity_log.entries().len(),
            log_len_before,
            "no log entry for a no-op call"
        );
    }

    #[test]
    fn trigger_manual_pr_retry_only_matching_issue_mutated() {
        let mut app = crate::tui::make_test_app("pr-retry-isolation");
        app.pending_prs.push(make_pending_pr(10));
        app.pending_prs.push(make_pending_pr(20));

        app.trigger_manual_pr_retry(10);

        assert_eq!(app.pending_prs[0].status, PendingPrStatus::RetryScheduled);
        assert_eq!(
            app.pending_prs[1].status,
            PendingPrStatus::AwaitingManualRetry,
            "issue 20 must be untouched"
        );
    }

    // ── Issue #521 follow-up: deterministic-failure exit ──────────────────

    #[test]
    fn pending_pr_transitions_to_permanently_failed_after_three_identical_errors() {
        let mut p = make_pending_pr(1);
        let err = "gh command failed: unknown flag: --json".to_string();
        for _ in 0..PENDING_PR_LAST_ERRORS_CAP {
            record_error_for_correlation(&mut p, err.clone());
        }
        assert_eq!(p.last_errors.len(), PENDING_PR_LAST_ERRORS_CAP);
        assert!(
            errors_match_threshold(&p),
            "three identical errors must trip the threshold"
        );
    }

    #[test]
    fn pending_pr_does_not_transition_when_errors_differ() {
        let mut p = make_pending_pr(1);
        record_error_for_correlation(&mut p, "first".into());
        record_error_for_correlation(&mut p, "second".into());
        record_error_for_correlation(&mut p, "third".into());
        assert_eq!(p.last_errors.len(), PENDING_PR_LAST_ERRORS_CAP);
        assert!(
            !errors_match_threshold(&p),
            "differing errors must NOT trip the threshold"
        );
    }

    #[test]
    fn pending_pr_last_errors_evicts_oldest_at_cap() {
        let mut p = make_pending_pr(1);
        for i in 0..(PENDING_PR_LAST_ERRORS_CAP + 2) {
            record_error_for_correlation(&mut p, format!("err-{}", i));
        }
        assert_eq!(p.last_errors.len(), PENDING_PR_LAST_ERRORS_CAP);
        // The oldest two ("err-0", "err-1") were evicted; "err-2..err-N" remain.
        assert_eq!(p.last_errors.front().unwrap(), "err-2");
        assert_eq!(
            p.last_errors.back().unwrap(),
            &format!("err-{}", PENDING_PR_LAST_ERRORS_CAP + 1)
        );
    }

    #[test]
    fn manual_retry_count_caps_at_lifetime_and_transitions_to_permanently_failed() {
        let mut app = crate::tui::make_test_app("manual-cap");
        app.pending_prs.push(make_pending_pr(42));

        // First N presses should keep queueing retries.
        for _ in 0..PENDING_PR_MANUAL_RETRY_LIFETIME_CAP {
            app.trigger_manual_pr_retry(42);
            assert_eq!(
                app.pending_prs[0].status,
                PendingPrStatus::RetryScheduled,
                "retries within the cap must keep queueing"
            );
        }
        assert_eq!(
            app.pending_prs[0].manual_retry_count,
            PENDING_PR_MANUAL_RETRY_LIFETIME_CAP
        );

        // (CAP + 1)th press transitions to PermanentlyFailed.
        app.trigger_manual_pr_retry(42);
        assert_eq!(
            app.pending_prs[0].status,
            PendingPrStatus::PermanentlyFailed,
            "press beyond the lifetime cap must transition to PermanentlyFailed"
        );
        let last = app
            .activity_log
            .entries()
            .last()
            .expect("log must contain transition entry");
        assert_eq!(last.level, LogLevel::Error);
        assert!(
            last.message.contains("Manual PR retries exhausted"),
            "got: {}",
            last.message
        );
    }

    #[test]
    fn permanently_failed_entries_are_skipped_by_trigger_manual_pr_retry() {
        let mut app = crate::tui::make_test_app("manual-skip-permfail");
        let mut p = make_pending_pr(7);
        p.status = PendingPrStatus::PermanentlyFailed;
        p.manual_retry_count = PENDING_PR_MANUAL_RETRY_LIFETIME_CAP + 5;
        app.pending_prs.push(p);
        let log_len_before = app.activity_log.entries().len();
        let count_before = app.pending_prs[0].manual_retry_count;

        app.trigger_manual_pr_retry(7);

        assert_eq!(
            app.pending_prs[0].status,
            PendingPrStatus::PermanentlyFailed,
            "PermanentlyFailed entries must not be re-queued"
        );
        assert_eq!(
            app.pending_prs[0].manual_retry_count, count_before,
            "manual_retry_count must NOT increment for PermanentlyFailed entries"
        );
        assert_eq!(
            app.activity_log.entries().len(),
            log_len_before,
            "no log entry for a skipped PermanentlyFailed entry"
        );
    }

    #[tokio::test]
    async fn permanently_failed_entries_are_skipped_by_process_pending_pr_retries() {
        let mut app = crate::tui::make_test_app("auto-skip-permfail");
        let mut p = make_pending_pr(99);
        p.status = PendingPrStatus::PermanentlyFailed;
        // Even with next_retry_at = "now" the entry must NOT be retried.
        p.next_retry_at = Some(chrono::Utc::now());
        let original_attempt = p.attempt;
        app.pending_prs.push(p);

        app.process_pending_pr_retries().await;

        assert_eq!(
            app.pending_prs[0].status,
            PendingPrStatus::PermanentlyFailed,
            "PermanentlyFailed must remain after a tick"
        );
        assert_eq!(
            app.pending_prs[0].attempt, original_attempt,
            "attempt counter must not advance for PermanentlyFailed"
        );
    }
}
