use super::App;
use crate::provider::github::ci::{CiCheck, CiChecker, CiStatus};
use crate::tui::activity_log::LogLevel;
use std::time::{Duration, Instant};

impl App {
    pub(super) fn poll_ci_status(&mut self) {
        let ci_poll_interval = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.gates.ci_poll_interval_secs))
            .unwrap_or(Duration::from_secs(30));

        let ci_max_wait = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.gates.ci_max_wait_secs))
            .unwrap_or(Duration::from_secs(1800));

        if self.last_ci_poll.elapsed() < ci_poll_interval || self.pending_pr_checks.is_empty() {
            return;
        }
        self.last_ci_poll = Instant::now();

        let auto_fix_enabled = self.flags.is_enabled(crate::flags::Flag::CiAutoFix);
        let max_retries = self
            .config
            .as_ref()
            .map(|c| c.gates.ci_auto_fix.max_retries)
            .unwrap_or(3);

        let checker = CiChecker::new();
        let mut completed_indices = Vec::new();
        // Collect fix requests to process after the loop (avoids borrow conflict)
        let mut fix_requests: Vec<crate::provider::github::ci::CiFixRequest> = Vec::new();
        let mut detail_updates: Vec<(u64, Vec<crate::provider::github::ci::CheckRunDetail>)> = Vec::new();

        for (i, check) in self.pending_pr_checks.iter_mut().enumerate() {
            check.check_count += 1;

            // Timeout check
            if check.created_at.elapsed() > ci_max_wait {
                self.activity_log.push_simple(
                    format!("PR #{}", check.pr_number),
                    format!(
                        "CI timed out after {}s",
                        check.created_at.elapsed().as_secs()
                    ),
                    LogLevel::Error,
                );
                completed_indices.push(i);
                continue;
            }

            // If awaiting a fix session's CI re-run, handle separately
            if check.awaiting_fix_ci {
                match checker.check_pr_status(check.pr_number) {
                    Ok(CiStatus::Pending) => {
                        // Fix was pushed, CI is re-running. Reset flag.
                        check.awaiting_fix_ci = false;
                    }
                    Ok(CiStatus::Passed) => {
                        check.awaiting_fix_ci = false;
                        self.activity_log.push_simple(
                            format!("PR #{}", check.pr_number),
                            format!("CI passed after {} fix attempt(s)", check.fix_attempt),
                            LogLevel::Info,
                        );
                        self.notifications.notify(
                            crate::notifications::types::InterruptLevel::Info,
                            &format!("PR #{}", check.pr_number),
                            "CI checks passed after auto-fix",
                        );
                        completed_indices.push(i);
                    }
                    Ok(CiStatus::Failed { .. }) => {
                        // Still showing old failure or fix didn't push yet. Keep waiting.
                    }
                    _ => {}
                }
                continue;
            }

            match checker.check_pr_status(check.pr_number) {
                Ok(CiStatus::Passed) => {
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        "CI passed".into(),
                        LogLevel::Info,
                    );
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Info,
                        &format!("PR #{}", check.pr_number),
                        "CI checks passed",
                    );
                    completed_indices.push(i);

                    // Auto-merge if configured
                    if let Some(ref config) = self.config
                        && config.github.auto_merge
                    {
                        let method_flag = config.github.merge_method.flag();
                        let pr_str = check.pr_number.to_string();
                        let result = std::process::Command::new("gh")
                            .args(["pr", "merge", &pr_str, method_flag, "--delete-branch"])
                            .output();
                        match result {
                            Ok(output) if output.status.success() => {
                                self.activity_log.push_simple(
                                    format!("PR #{}", check.pr_number),
                                    "Auto-merged".into(),
                                    LogLevel::Info,
                                );
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                self.activity_log.push_simple(
                                    format!("PR #{}", check.pr_number),
                                    format!("Auto-merge failed: {}", stderr.trim()),
                                    LogLevel::Error,
                                );
                            }
                            Err(e) => {
                                self.activity_log.push_simple(
                                    format!("PR #{}", check.pr_number),
                                    format!("Auto-merge error: {}", e),
                                    LogLevel::Error,
                                );
                            }
                        }
                    }
                }
                Ok(CiStatus::Failed { summary }) => {
                    use crate::provider::github::ci::{CiFixRequest, CiPollAction, decide_ci_action};
                    if let Ok(details) = checker.check_pr_details(check.pr_number) {
                        detail_updates.push((check.pr_number, details));
                    }

                    let action = if auto_fix_enabled {
                        decide_ci_action(check, max_retries, &summary)
                    } else {
                        CiPollAction::Abandon
                    };

                    match action {
                        CiPollAction::SpawnFix { .. } => {
                            match checker.fetch_failure_log(check.pr_number, &check.branch) {
                                Ok(failure_log) => {
                                    self.activity_log.push_simple(
                                        format!("PR #{}", check.pr_number),
                                        format!(
                                            "CI failed (attempt {}/{}), spawning fix session",
                                            check.fix_attempt + 1,
                                            max_retries
                                        ),
                                        LogLevel::Warn,
                                    );
                                    fix_requests.push(CiFixRequest {
                                        pr_number: check.pr_number,
                                        issue_number: check.issue_number,
                                        branch: check.branch.clone(),
                                        attempt: check.fix_attempt + 1,
                                        failure_log,
                                    });
                                    check.fix_attempt += 1;
                                    check.awaiting_fix_ci = true;
                                }
                                Err(e) => {
                                    self.activity_log.push_simple(
                                        format!("PR #{}", check.pr_number),
                                        format!("CI failed, could not fetch log: {}", e),
                                        LogLevel::Error,
                                    );
                                    completed_indices.push(i);
                                }
                            }
                        }
                        CiPollAction::Abandon => {
                            self.activity_log.push_simple(
                                format!("PR #{}", check.pr_number),
                                if auto_fix_enabled {
                                    format!(
                                        "CI failed after {} fix attempts: {}",
                                        check.fix_attempt, summary
                                    )
                                } else {
                                    format!("CI failed: {}", summary)
                                },
                                LogLevel::Error,
                            );
                            self.notifications.notify(
                                crate::notifications::types::InterruptLevel::Critical,
                                &format!("PR #{} CI failed", check.pr_number),
                                &summary,
                            );
                            completed_indices.push(i);
                        }
                        CiPollAction::Wait => {} // awaiting_fix_ci handled above
                    }
                }
                Ok(CiStatus::NoneConfigured) => {
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        "No CI checks configured".into(),
                        LogLevel::Info,
                    );
                    completed_indices.push(i);
                }
                Ok(CiStatus::Pending) => {
                    if let Ok(details) = checker.check_pr_details(check.pr_number) {
                        detail_updates.push((check.pr_number, details));
                    }
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        format!("CI check error: {}", e),
                        LogLevel::Error,
                    );
                    // Don't remove — will retry next poll
                }
            }
        }

        // Spawn fix sessions after the loop to avoid borrow conflicts
        for req in fix_requests {
            self.spawn_ci_fix_session(
                req.pr_number,
                req.issue_number,
                req.branch,
                req.attempt,
                &req.failure_log,
            );
        }

        // Update CI check details for TUI display
        for (pr_number, details) in detail_updates {
            self.ci_check_details.insert(pr_number, details);
        }

        // Remove completed checks in reverse order to preserve indices
        completed_indices.sort_unstable();
        for &i in &completed_indices {
            let pr_number = self.pending_pr_checks[i].pr_number;
            self.ci_check_details.remove(&pr_number);
        }
        for i in completed_indices.into_iter().rev() {
            self.pending_pr_checks.remove(i);
        }
    }
}
