//! #1153 — apply half of the auto-PR pipeline. Every App mutation
//! implied by a [`PrPhaseOutcome`]: activity logs, notifications, CI
//! poller inserts, PendingPr enqueues, pool transitions, review
//! dispatch and the PrCreated hook. The network/git decisions live in
//! `auto_pr_phase.rs`; this file only replays them onto App state.

use super::App;
use super::auto_pr_phase::PrPhaseOutcome;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::provider::github::ci::PendingPrCheck;
use crate::provider::github::pr::PrRetryPolicy;
use crate::provider::github::types::{PendingPr, PendingPrStatus};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;
use crate::util::sanitize::sanitize_log;
use std::time::Instant;

/// Context the apply half needs but the phase cannot carry.
pub(super) struct PrApplyCtx {
    pub issue_number: u64,
    pub issue_numbers: Vec<u64>,
    pub cost_usd: f64,
    pub branch: Option<String>,
    pub base_branch: String,
}

impl App {
    /// Apply the phase outcome: every activity-log line, notification,
    /// poller insert, PendingPr enqueue, pool transition, review
    /// dispatch and plugin hook the pre-split inline code executed,
    /// in the same order.
    pub(super) async fn apply_pr_outcome(&mut self, outcome: PrPhaseOutcome, ctx: PrApplyCtx) {
        match outcome {
            PrPhaseOutcome::Disabled { branch } => {
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    format!(
                        "Auto-PR disabled in config (github.auto_pr = false). Branch: {}",
                        branch.as_deref().unwrap_or("(none)")
                    ),
                    LogLevel::Info,
                );
            }
            PrPhaseOutcome::NoBranch => {
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    format!(
                        "PR creation skipped — session #{} has no worktree branch.",
                        ctx.issue_number
                    ),
                    LogLevel::Error,
                );
            }
            PrPhaseOutcome::NoClient => {
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    "PR creation skipped — GitHub client not initialized.".to_string(),
                    LogLevel::Error,
                );
            }
            PrPhaseOutcome::SkippedNotSuccess | PrPhaseOutcome::AlreadyAttempted => {}
            PrPhaseOutcome::AuthSkippedNoBranch => {
                self.log_gh_auth_skip(ctx.issue_number, "PR creation");
            }
            PrPhaseOutcome::AuthDeferred { branch, diff_files } => {
                self.apply_auth_deferred_pr(
                    ctx.issue_number,
                    ctx.issue_numbers,
                    ctx.cost_usd,
                    branch,
                    diff_files,
                );
            }
            PrPhaseOutcome::ExistingPr {
                pr_number,
                url,
                branch,
            } => {
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    format!("PR already exists for branch {} — see {}", branch, url),
                    LogLevel::Info,
                );
                self.ci_poller.add_check(PendingPrCheck {
                    pr_number,
                    issue_number: ctx.issue_number,
                    branch,
                    created_at: Instant::now(),
                    check_count: 0,
                    fix_attempt: 0,
                    awaiting_fix_ci: false,
                });
            }
            PrPhaseOutcome::NoCommits { branch } => {
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    format!(
                        "No commits found — skipping PR creation. Branch: {}",
                        branch
                    ),
                    LogLevel::Warn,
                );
                self.notifications.notify(
                    crate::notifications::types::InterruptLevel::Critical,
                    &format!("#{} — PR not opened", ctx.issue_number),
                    &format!(
                        "Session ended with zero commits on branch {}; no PR was created.",
                        branch
                    ),
                );
            }
            PrPhaseOutcome::FetchFailed { error } => {
                self.check_gh_auth_error(&error);
                let err_text = sanitize_log(&error.to_string());
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    format!(
                        "PR creation skipped — could not fetch issue from GitHub: {}",
                        err_text
                    ),
                    LogLevel::Error,
                );
                self.notifications.notify(
                    crate::notifications::types::InterruptLevel::Critical,
                    &format!("#{} — PR not opened", ctx.issue_number),
                    &format!(
                        "Maestro could not draft the PR because the issue is not in cache and the GitHub fetch failed: {}",
                        err_text
                    ),
                );
            }
            PrPhaseOutcome::Created {
                pr_number,
                url,
                branch,
            } => {
                self.activity_log.push_simple(
                    format!("#{}", ctx.issue_number),
                    format!("PR #{} created: {}", pr_number, url),
                    LogLevel::Info,
                );
                self.ci_poller.add_check(PendingPrCheck {
                    pr_number,
                    issue_number: ctx.issue_number,
                    branch: branch.clone(),
                    created_at: Instant::now(),
                    check_count: 0,
                    fix_attempt: 0,
                    awaiting_fix_ci: false,
                });
                self.dispatch_review(pr_number, &branch, ctx.issue_number);
                let session_id = self
                    .pool
                    .find_by_issue_mut(ctx.issue_number)
                    .map(|m| m.session.id.to_string())
                    .unwrap_or_default();
                let hook_ctx = HookContext::new()
                    .with_session(&session_id, Some(ctx.issue_number))
                    .with_pr(pr_number)
                    .with_branch(&branch)
                    .with_cost(ctx.cost_usd);
                self.fire_plugin_hook(HookPoint::PrCreated, hook_ctx).await;
            }
            PrPhaseOutcome::Failed { error, diff_files } => {
                let was_auth = self.check_gh_auth_error(&error);
                let policy = PrRetryPolicy::default();
                let now = chrono::Utc::now();
                let mut last_errors = std::collections::VecDeque::new();
                last_errors.push_back(error.to_string());
                let pending = PendingPr {
                    issue_number: ctx.issue_number,
                    issue_numbers: ctx.issue_numbers.clone(),
                    branch: ctx.branch.clone().unwrap_or_default(),
                    base_branch: ctx.base_branch.clone(),
                    // Field name predates #1142; the payload is now the
                    // repo-relative diff list the retry body renders.
                    files_touched: diff_files,
                    cost_usd: ctx.cost_usd,
                    attempt: 0,
                    max_attempts: policy.max_attempts,
                    last_attempt_at: now,
                    next_retry_at: policy.delay_for_attempt(0).map(|d| {
                        now + chrono::Duration::from_std(d).unwrap_or(chrono::Duration::seconds(5))
                    }),
                    status: PendingPrStatus::RetryScheduled,
                    last_errors,
                    manual_retry_count: 0,
                };
                self.pending_prs.push(pending);

                if let Some(managed) = self.pool.find_by_issue_mut(ctx.issue_number) {
                    let _ = managed
                        .session
                        .transition_to(SessionStatus::NeedsPr, TransitionReason::PrNeeded);
                }

                if !was_auth {
                    let err_text = sanitize_log(&error.to_string());
                    self.activity_log.push_simple(
                        format!("#{}", ctx.issue_number),
                        format!(
                            "PR creation failed for branch {}: {}. Retrying automatically; \
                             run `gh pr create --base {} --head {}` to create manually.",
                            ctx.branch.as_deref().unwrap_or_default(),
                            err_text,
                            ctx.base_branch,
                            ctx.branch.as_deref().unwrap_or_default()
                        ),
                        LogLevel::Warn,
                    );
                }
            }
        }
    }

    /// Auth-missing deferral half of the pre-split
    /// `defer_pr_for_missing_auth` (#521): dedupe against the queue,
    /// enqueue an `AwaitingManualRetry` entry, log the skip.
    fn apply_auth_deferred_pr(
        &mut self,
        issue_number: u64,
        issue_numbers: Vec<u64>,
        cost_usd: f64,
        branch: String,
        diff_files: Vec<String>,
    ) {
        let already_queued = self
            .pending_prs
            .iter()
            .any(|p| p.issue_number == issue_number);
        if already_queued {
            self.log_gh_auth_skip(issue_number, "PR creation");
            return;
        }
        let base_branch = self
            .config
            .as_ref()
            .map(|c| c.project.base_branch.clone())
            .unwrap_or_else(|| "main".to_string());
        let mut last_errors = std::collections::VecDeque::new();
        last_errors.push_back(format!(
            "GitHub auth missing. {}",
            super::AUTH_RECOVERY_HINT
        ));
        let pending = PendingPr {
            issue_number,
            issue_numbers,
            branch,
            base_branch,
            files_touched: diff_files,
            cost_usd,
            attempt: 0,
            max_attempts: PrRetryPolicy::default().max_attempts,
            last_attempt_at: chrono::Utc::now(),
            next_retry_at: None,
            status: PendingPrStatus::AwaitingManualRetry,
            last_errors,
            manual_retry_count: 0,
        };
        self.pending_prs.push(pending);

        self.activity_log.push_simple(
            format!("#{}", issue_number),
            format!(
                "PR creation deferred — auth missing. {}",
                super::AUTH_RECOVERY_HINT
            ),
            LogLevel::Warn,
        );
    }
}
