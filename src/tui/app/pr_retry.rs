use super::App;
use crate::github::ci::PendingPrCheck;
use crate::github::pr::{PrCreator, PrRetryPolicy};
use crate::github::types::PendingPrStatus;
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
                        self.pending_pr_checks.push(PendingPrCheck {
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
                        let pending = &mut self.pending_prs[idx];
                        pending.last_error = e.to_string();
                        if let Some(delay) = policy.delay_for_attempt(attempt) {
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
    pub fn trigger_manual_pr_retry(&mut self, issue_number: u64) {
        if let Some(pending) = self
            .pending_prs
            .iter_mut()
            .find(|p| p.issue_number == issue_number)
        {
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
}
