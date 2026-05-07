//! Manual CI Error Review entry points (#695).
//!
//! Bridges the CI monitor (failure rendering + key event) to the
//! popup screen and the existing `spawn_ci_fix_session` machinery.

use super::App;
use super::types::{TuiCommand, TuiMode};
use crate::provider::github::ci::local_gate_for_check;
use crate::provider::github::redaction::redact_secrets;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::{
    CiErrorReviewScreen, CiErrorReviewState, CiFixConfig, FetchPhase, sanitize_for_terminal,
};

/// Snapshot of the candidate PR selected for the manual error review.
#[derive(Debug, Clone)]
pub(crate) struct FailingPrSnapshot {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub awaiting_fix_ci: bool,
    pub failed_check_names: Vec<String>,
}

impl App {
    /// True if at least one tracked PR has a failed check rendered.
    pub fn has_visible_ci_failure(&self) -> bool {
        self.ci_poller
            .ci_check_details
            .values()
            .any(|checks| checks.iter().any(|c| c.is_failure()))
    }

    /// Pick the lowest-numbered PR with at least one failed check.
    pub(crate) fn first_failing_pr(&self) -> Option<FailingPrSnapshot> {
        let (pr_number, checks) = self
            .ci_poller
            .ci_check_details
            .iter()
            .filter(|(_, checks)| checks.iter().any(|c| c.is_failure()))
            .min_by_key(|(pr, _)| **pr)?;
        let pending = self
            .ci_poller
            .pending_pr_checks
            .iter()
            .find(|c| c.pr_number == *pr_number)?;
        let failed_check_names: Vec<String> = checks
            .iter()
            .filter(|c| c.is_failure())
            .map(|c| c.name.clone())
            .collect();
        Some(FailingPrSnapshot {
            pr_number: *pr_number,
            issue_number: pending.issue_number,
            branch: pending.branch.clone(),
            awaiting_fix_ci: pending.awaiting_fix_ci,
            failed_check_names,
        })
    }

    /// Open the CI Error Review popup for the lowest-numbered failing PR.
    /// Refuses (no-op + log) if a fix session is already in flight, or if
    /// the popup is already open (rapid `[e]` repeats must not re-fetch).
    pub fn request_ci_error_review(&mut self) {
        if self.tui_mode == TuiMode::CiErrorReview
            || self.screen_state.ci_error_review_screen.is_some()
        {
            return;
        }
        let Some(snapshot) = self.first_failing_pr() else {
            return;
        };
        if snapshot.awaiting_fix_ci {
            self.activity_log.push_simple(
                format!("PR #{}", snapshot.pr_number),
                "Fix already in progress for this PR; wait for it to push.".into(),
                LogLevel::Info,
            );
            return;
        }
        let planned_gate_cmd = snapshot
            .failed_check_names
            .first()
            .and_then(|name| local_gate_for_check(name).map(|c| c.to_string()));
        let sanitized_names: Vec<String> = snapshot
            .failed_check_names
            .iter()
            .map(|n| sanitize_for_terminal(n))
            .collect();
        let state = CiErrorReviewState {
            pr_number: snapshot.pr_number,
            issue_number: snapshot.issue_number,
            branch: sanitize_for_terminal(&snapshot.branch),
            failed_check_names: sanitized_names,
            planned_gate_cmd,
            fetch: FetchPhase::Loading,
        };
        self.screen_state.ci_error_review_screen = Some(CiErrorReviewScreen::new(state));
        self.tui_mode = TuiMode::CiErrorReview;
        self.pending_commands.push(TuiCommand::FetchCiErrorReview {
            pr_number: snapshot.pr_number,
            branch: snapshot.branch,
        });
    }

    /// Apply a fetched-log result to the open review screen.
    pub fn handle_ci_error_review_fetched(
        &mut self,
        pr_number: u64,
        result: Result<String, String>,
    ) {
        let Some(screen) = self.screen_state.ci_error_review_screen.as_mut() else {
            return;
        };
        if screen.state.pr_number != pr_number {
            return;
        }
        // Redact secret-shaped tokens, then strip terminal control bytes.
        // The log is rendered to the terminal AND embedded in the agent
        // prompt, so both layers benefit from sanitization.
        screen.state.fetch = match result {
            Ok(log_excerpt) => FetchPhase::Ready {
                log_excerpt: sanitize_for_terminal(&redact_secrets(&log_excerpt)),
            },
            Err(reason) => FetchPhase::Failed {
                reason: sanitize_for_terminal(&reason),
            },
        };
    }

    /// Confirm the review and queue the fix session via the shared
    /// `spawn_ci_fix_session` path. Sets `awaiting_fix_ci=true` on the
    /// matching `PendingPrCheck` BEFORE spawning, so the auto path on a
    /// later tick cannot also queue a fix for the same PR.
    pub(crate) fn launch_ci_fix_from_review(&mut self, config: &CiFixConfig) {
        let Some(check) = self
            .ci_poller
            .pending_pr_checks
            .iter_mut()
            .find(|c| c.pr_number == config.pr_number)
        else {
            self.activity_log.push_simple(
                format!("PR #{}", config.pr_number),
                "Cannot launch fix: PR no longer tracked.".into(),
                LogLevel::Warn,
            );
            return;
        };
        check.awaiting_fix_ci = true;
        check.fix_attempt += 1;
        let attempt = check.fix_attempt;
        self.spawn_ci_fix_session_with_gate(
            config.pr_number,
            config.issue_number,
            config.branch.clone(),
            attempt,
            &config.failure_log,
            config.local_gate_cmd.as_deref(),
        );
    }
}

#[cfg(test)]
#[path = "ci_error_review_tests.rs"]
mod tests_split;
