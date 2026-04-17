use super::App;
use super::types::TuiMode;
use crate::provider::github::ci::PendingPrCheck;
use crate::provider::github::labels::LabelManager;
use crate::provider::github::pr::{PrCreator, PrRetryPolicy};
use crate::provider::github::types::{PendingPr, PendingPrStatus};
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;
use std::time::Instant;

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
        if let Some(ref mut assigner) = self.work_assigner {
            if success {
                let unblocked = assigner.mark_done(issue_number);
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
                let cascaded = assigner.mark_failed_cascade(issue_number);
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
        } else if success {
            // Extract config values before entering async/mutable code
            let auto_pr = self
                .config
                .as_ref()
                .map(|c| c.github.auto_pr)
                .unwrap_or(false);
            let base_branch = self
                .config
                .as_ref()
                .map(|c| c.project.base_branch.clone())
                .unwrap_or_else(|| "main".to_string());

            if auto_pr
                && self.github_client.is_some()
                && let Some(ref branch) = worktree_branch
                && let Some(issue) = self.state.issue_cache.get(&issue_number)
            {
                let file_refs: Vec<&str> = files_touched.iter().map(|s| s.as_str()).collect();
                let client = self.github_client.as_ref().unwrap();
                let pr_creator = PrCreator::new(client.as_ref(), base_branch.clone());

                let pr_result = if is_unified {
                    let unified_issues: Vec<&crate::provider::github::types::GhIssue> = issue_numbers
                        .iter()
                        .filter_map(|n| self.state.issue_cache.get(n))
                        .collect();
                    if unified_issues.is_empty() {
                        pr_creator
                            .create_for_issue(issue, branch, &file_refs, cost_usd)
                            .await
                    } else {
                        let body = crate::provider::github::pr::build_unified_pr_body(
                            &unified_issues,
                            &file_refs,
                            cost_usd,
                        );
                        let refs: Vec<String> =
                            issue_numbers.iter().map(|n| format!("#{}", n)).collect();
                        let title = format!("[Maestro] Unified: {}", refs.join(", "));
                        client
                            .create_pr(issue_number, &title, &body, branch, &base_branch)
                            .await
                    }
                } else {
                    pr_creator
                        .create_for_issue(issue, branch, &file_refs, cost_usd)
                        .await
                };

                match pr_result {
                    Ok(pr_num) => {
                        self.activity_log.push_simple(
                            format!("#{}", issue_number),
                            format!("PR #{} created", pr_num),
                            LogLevel::Info,
                        );
                        // Track PR for CI polling
                        if let Some(ref branch_name) = worktree_branch {
                            self.pending_pr_checks.push(PendingPrCheck {
                                pr_number: pr_num,
                                issue_number,
                                branch: branch_name.clone(),
                                created_at: Instant::now(),
                                check_count: 0,
                                fix_attempt: 0,
                                awaiting_fix_ci: false,
                            });
                        }
                        self.dispatch_review(pr_num, branch, issue_number);
                        // Fire pr_created hook
                        let ctx = HookContext::new()
                            .with_session("", Some(issue_number))
                            .with_pr(pr_num)
                            .with_branch(branch)
                            .with_cost(cost_usd);
                        self.fire_plugin_hook(HookPoint::PrCreated, ctx).await;
                    }
                    Err(e) => {
                        self.check_gh_auth_error(&e);
                        let policy = PrRetryPolicy::default();
                        let now = chrono::Utc::now();
                        let pending = PendingPr {
                            issue_number,
                            issue_numbers: issue_numbers.clone(),
                            branch: branch.clone(),
                            base_branch: base_branch.clone(),
                            files_touched: files_touched.clone(),
                            cost_usd,
                            attempt: 0,
                            max_attempts: policy.max_attempts,
                            last_error: e.to_string(),
                            last_attempt_at: now,
                            next_retry_at: policy.delay_for_attempt(0).map(|d| {
                                now + chrono::Duration::from_std(d)
                                    .unwrap_or(chrono::Duration::seconds(5))
                            }),
                            status: PendingPrStatus::RetryScheduled,
                        };
                        self.pending_prs.push(pending);

                        // Update session status to NeedsPr
                        if let Some(managed) = self.pool.find_by_issue_mut(issue_number) {
                            let _ = managed
                                .session
                                .transition_to(SessionStatus::NeedsPr, TransitionReason::PrNeeded);
                        }

                        self.activity_log.push_simple(
                            format!("#{}", issue_number),
                            format!("PR creation failed (will retry): {}", e),
                            LogLevel::Warn,
                        );
                    }
                }
            }
        }
    }
}
