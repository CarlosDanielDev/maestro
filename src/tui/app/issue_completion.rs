use super::App;
use super::types::TuiMode;
use crate::provider::github::labels::LabelManager;
use crate::tui::activity_log::LogLevel;

impl App {
    #[allow(clippy::too_many_arguments)]
    pub async fn on_issue_session_completed(
        &mut self,
        issue_number: u64,
        issue_numbers: Vec<u64>,
        success: bool,
        cost_usd: f64,
        files_touched: Vec<String>,
        worktree_branch: Option<String>,
        is_ci_fix: bool,
    ) {
        let is_unified = issue_numbers.len() >= 2;
        // Update work assigner
        if let Some(ref mut service) = self.work_assignment_service {
            if success {
                let unblocked = service.inner_mut().mark_done(issue_number);
                if !unblocked.is_empty() {
                    let nums: Vec<String> = unblocked
                        .iter()
                        .map(|i| format!("#{}", i.number()))
                        .collect();
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Unblocked: {}", nums.join(", ")),
                        LogLevel::Info,
                    );
                }
            } else {
                let cascaded = service.inner_mut().mark_failed_cascade(issue_number);
                if !cascaded.is_empty() {
                    let nums: Vec<String> = cascaded.iter().map(|n| format!("#{}", n)).collect();
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Cascade failed: {}", nums.join(", ")),
                        LogLevel::Error,
                    );
                    // Emit critical notification for cascaded failures
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Critical,
                        &format!("#{} failed", issue_number),
                        &format!(
                            "Blocked {} dependent task{}: {}",
                            cascaded.len(),
                            if cascaded.len() != 1 { "s" } else { "" },
                            nums.join(", ")
                        ),
                    );
                }
            }
        }

        // Continuous mode: track completion/failure
        if let Some(ref mut cont) = self.continuous_mode {
            if success {
                cont.on_issue_completed(issue_number);
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!(
                        "Issue #{} completed ({} done so far)",
                        issue_number, cont.completed_count
                    ),
                    LogLevel::Info,
                );
            } else {
                let title = self
                    .state
                    .issue_cache
                    .get(&issue_number)
                    .map(|i| i.title.clone())
                    .unwrap_or_else(|| format!("Issue #{}", issue_number));
                let entries = self.activity_log.entries();
                let error_summary = entries
                    .iter()
                    .rev()
                    .take(10)
                    .find(|e| e.level == LogLevel::Error)
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "Session failed".into());
                cont.on_issue_failed(issue_number, title, error_summary);
                self.tui_mode = TuiMode::ContinuousPause;
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Issue #{} failed — paused for user decision", issue_number),
                    LogLevel::Warn,
                );
            }
        }

        // Update GitHub labels (for all issues in unified sessions)
        if let Some(ref client) = self.github_client {
            if !self.gh_auth_ok {
                self.log_gh_auth_skip(issue_number, "label update");
            } else {
                let label_mgr = LabelManager::new(client.as_ref());
                let issues_to_label = if is_unified {
                    issue_numbers.clone()
                } else {
                    vec![issue_number]
                };
                let mut label_errors: Vec<(u64, anyhow::Error)> = Vec::new();
                for num in &issues_to_label {
                    let result = if success {
                        label_mgr.mark_done(*num).await
                    } else {
                        label_mgr.mark_failed(*num).await
                    };
                    if let Err(e) = result {
                        label_errors.push((*num, e));
                    }
                }
                for (num, e) in label_errors {
                    self.check_gh_auth_error(&e);
                    self.activity_log.push_simple(
                        format!("#{}", num),
                        format!("Label update failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        }

        // CI fix sessions skip PR creation — the PR already exists
        if is_ci_fix {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "CI fix pushed to existing PR branch".into(),
                LogLevel::Info,
            );
            return;
        }

        // Auto PR creation (skip if auth lost)
        if !self.gh_auth_ok {
            if success {
                self.log_gh_auth_skip(issue_number, "PR creation");
            }
            return;
        }
        if !success {
            return;
        }

        self.run_auto_pr(
            issue_number,
            issue_numbers,
            cost_usd,
            files_touched,
            worktree_branch,
            is_unified,
        )
        .await;
    }
}

#[cfg(test)]
#[path = "issue_completion_tests.rs"]
mod tests;
