use super::App;
use super::types::TuiDataEvent;
use crate::session::types::Session;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::milestone::MilestoneEntry;

impl App {
    /// Resolve model and mode from config and issue labels.
    fn resolve_model_and_mode(&self, labels: &[String]) -> (String, String) {
        let model = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_model.clone())
            .unwrap_or_else(|| "opus".to_string());
        let default_mode = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_mode.clone())
            .unwrap_or_else(|| "orchestrator".to_string());
        let mode = crate::modes::mode_from_labels(labels).unwrap_or(default_mode);
        (model, mode)
    }

    /// Build a prompt from issue + optional custom instructions.
    fn build_issue_prompt_with_custom(
        &self,
        gh_issue: &crate::provider::github::types::GhIssue,
        custom_prompt: &Option<String>,
    ) -> String {
        let base = self
            .config
            .as_ref()
            .map(|c| crate::prompts::PromptBuilder::build_issue_prompt(gh_issue, c))
            .unwrap_or_else(|| gh_issue.unattended_prompt());
        match custom_prompt {
            Some(cp) if !cp.trim().is_empty() => {
                format!("{}\n\n## Additional Instructions\n\n{}", base, cp.trim())
            }
            _ => base,
        }
    }

    /// Process a data event from a background fetch task.
    pub fn handle_data_event(&mut self, evt: TuiDataEvent) {
        match evt {
            TuiDataEvent::Issues(Ok(issues)) => {
                if let Some(ref mut screen) = self.issue_browser_screen {
                    screen.set_issues(issues);
                }
            }
            TuiDataEvent::Issues(Err(e)) => {
                self.check_gh_auth_error(&e);
                self.activity_log.push_simple(
                    "Issues".into(),
                    format!("Failed to fetch issues: {}", e),
                    LogLevel::Error,
                );
                if let Some(ref mut screen) = self.issue_browser_screen {
                    screen.loading = false;
                }
            }
            TuiDataEvent::Milestones(Ok(entries)) => {
                if let Some(ref mut screen) = self.milestone_screen {
                    screen.milestones = entries.into_iter().map(MilestoneEntry::from).collect();
                    screen.loading = false;
                }
            }
            TuiDataEvent::Milestones(Err(e)) => {
                self.check_gh_auth_error(&e);
                self.activity_log.push_simple(
                    "Milestones".into(),
                    format!("Failed to fetch milestones: {}", e),
                    LogLevel::Error,
                );
                if let Some(ref mut screen) = self.milestone_screen {
                    screen.loading = false;
                }
            }
            TuiDataEvent::Issue(Ok(gh_issue), custom_prompt) => {
                let (model, issue_mode) = self.resolve_model_and_mode(&gh_issue.labels);
                let prompt = self.build_issue_prompt_with_custom(&gh_issue, &custom_prompt);
                let issue_number = gh_issue.number;
                let mut session = Session::new(prompt, model, issue_mode, Some(issue_number));
                session.issue_title = Some(gh_issue.title.clone());
                self.state.issue_cache.insert(issue_number, gh_issue);
                self.pending_session_launches.push(session);
            }
            TuiDataEvent::Issue(Err(e), _) => {
                self.activity_log.push_simple(
                    "Session".into(),
                    format!("Failed to fetch issue: {}", e),
                    LogLevel::Error,
                );
            }
            TuiDataEvent::SuggestionData(payload) => {
                let active = self.pool.active_count();
                let total = self.pool.total_count();
                let suggestions = crate::tui::screens::home::Suggestion::build_suggestions(
                    payload.ready_issue_count,
                    payload.failed_issue_count,
                    &payload.milestones,
                    active,
                );
                let milestone_active = payload.milestones.first().map(|(t, c, tot)| {
                    crate::tui::screens::home::MilestoneStats {
                        title: t.clone(),
                        closed: *c,
                        total: *tot,
                    }
                });
                let stats = crate::tui::screens::home::ProjectStats {
                    loaded: true,
                    issues_open: payload.open_issue_count,
                    issues_closed: payload.closed_issue_count,
                    milestone_active,
                    sessions_active: active,
                    sessions_total: total,
                };
                if let Some(ref mut screen) = self.home_screen {
                    screen.set_suggestions(suggestions);
                    screen.set_stats(stats);
                }
            }
            TuiDataEvent::VersionCheckResult(Some(info)) => {
                self.activity_log.push_simple(
                    "UPDATE".into(),
                    format!("New version {} available", info.tag),
                    LogLevel::Info,
                );
                self.upgrade_state = crate::updater::UpgradeState::Available(info);
            }
            TuiDataEvent::VersionCheckResult(None) => {}
            TuiDataEvent::UpgradeResult(Ok(backup_path)) => {
                if let crate::updater::UpgradeState::Downloading { version } = &self.upgrade_state {
                    self.upgrade_state = crate::updater::UpgradeState::ReadyToRestart {
                        version: version.clone(),
                        backup_path,
                    };
                }
            }
            TuiDataEvent::UpgradeResult(Err(msg)) => {
                self.upgrade_state = crate::updater::UpgradeState::Failed(msg);
            }
            TuiDataEvent::AdaptScanResult(result) => {
                if let Some(ref mut screen) = self.adapt_screen {
                    if screen.is_cancelled() {
                        return;
                    }
                    match result {
                        Ok(profile) => {
                            if let Some(cmd) = screen.complete_scan(*profile) {
                                self.pending_commands.push(cmd);
                            }
                        }
                        Err(e) => {
                            screen.set_error(
                                crate::tui::screens::adapt::AdaptStep::Scanning,
                                format!("{}", e),
                            );
                        }
                    }
                }
            }
            TuiDataEvent::AdaptAnalyzeResult(result) => {
                if let Some(ref mut screen) = self.adapt_screen {
                    if screen.is_cancelled() {
                        return;
                    }
                    match result {
                        Ok(report) => {
                            if let Some(cmd) = screen.complete_analyze(report) {
                                self.pending_commands.push(cmd);
                            }
                        }
                        Err(e) => {
                            screen.set_error(
                                crate::tui::screens::adapt::AdaptStep::Analyzing,
                                format!("{}", e),
                            );
                        }
                    }
                }
            }
            TuiDataEvent::AdaptConsolidateResult(result) => {
                if let Some(ref mut screen) = self.adapt_screen {
                    if screen.is_cancelled() {
                        return;
                    }
                    match result {
                        Ok(prd_content) => {
                            if let Some(cmd) = screen.complete_consolidate(prd_content) {
                                self.pending_commands.push(cmd);
                            }
                        }
                        Err(e) => {
                            screen.set_error(
                                crate::tui::screens::adapt::AdaptStep::Consolidating,
                                format!("{}", e),
                            );
                        }
                    }
                }
            }
            TuiDataEvent::AdaptPlanResult(result) => {
                if let Some(ref mut screen) = self.adapt_screen {
                    if screen.is_cancelled() {
                        return;
                    }
                    match result {
                        Ok(plan) => {
                            if let Some(cmd) = screen.complete_plan(plan) {
                                self.pending_commands.push(cmd);
                            }
                        }
                        Err(e) => {
                            screen.set_error(
                                crate::tui::screens::adapt::AdaptStep::Planning,
                                format!("{}", e),
                            );
                        }
                    }
                }
            }
            TuiDataEvent::PullRequests(Ok(prs)) => {
                if let Some(ref mut screen) = self.pr_review_screen {
                    screen.set_prs(prs);
                }
            }
            TuiDataEvent::PullRequests(Err(e)) => {
                self.check_gh_auth_error(&e);
                self.activity_log.push_simple(
                    "PRs".into(),
                    format!("Failed to fetch pull requests: {}", e),
                    LogLevel::Error,
                );
                if let Some(ref mut screen) = self.pr_review_screen {
                    screen.set_loading_error(&format!("{}", e));
                }
            }
            TuiDataEvent::PrReviewSubmitted(Ok(())) => {
                self.activity_log.push_simple(
                    "PR Review".into(),
                    "Review submitted successfully".into(),
                    LogLevel::Info,
                );
                if let Some(ref mut screen) = self.pr_review_screen {
                    screen.set_done();
                }
            }
            TuiDataEvent::PrReviewSubmitted(Err(e)) => {
                self.check_gh_auth_error(&e);
                self.activity_log.push_simple(
                    "PR Review".into(),
                    format!("Failed to submit review: {}", e),
                    LogLevel::Error,
                );
                if let Some(ref mut screen) = self.pr_review_screen {
                    screen.set_error(&format!("{}", e));
                }
            }
            TuiDataEvent::AdaptMaterializeResult(result) => {
                if let Some(ref mut screen) = self.adapt_screen {
                    if screen.is_cancelled() {
                        return;
                    }
                    match result {
                        Ok(mat_result) => {
                            screen.complete_materialize(mat_result);
                        }
                        Err(e) => {
                            screen.set_error(
                                crate::tui::screens::adapt::AdaptStep::Materializing,
                                format!("{}", e),
                            );
                        }
                    }
                }
            }
            TuiDataEvent::UnifiedIssues(Ok(gh_issues), custom_prompt) => {
                let first_labels = gh_issues
                    .first()
                    .map(|i| i.labels.as_slice())
                    .unwrap_or(&[]);
                let (model, issue_mode) = self.resolve_model_and_mode(first_labels);
                let issue_numbers: Vec<u64> = gh_issues.iter().map(|i| i.number).collect();

                let mut combined_prompt = String::from(
                    "You are working on multiple related issues in a single unified PR.\n\n",
                );
                for (i, issue) in gh_issues.iter().enumerate() {
                    if i > 0 {
                        combined_prompt.push_str("\n\n---\n\n");
                    }
                    combined_prompt.push_str(&self.build_issue_prompt_with_custom(issue, &None));
                }
                if let Some(ref cp) = custom_prompt
                    && !cp.trim().is_empty()
                {
                    combined_prompt
                        .push_str(&format!("\n\n## Additional Instructions\n\n{}", cp.trim()));
                }

                let primary_issue = issue_numbers.first().copied();
                let mut session = Session::new(combined_prompt, model, issue_mode, primary_issue);
                session.issue_numbers = issue_numbers;
                session.issue_title = Some(format!(
                    "Unified: {}",
                    gh_issues
                        .iter()
                        .map(|i| format!("#{}", i.number))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));

                for issue in &gh_issues {
                    self.state.issue_cache.insert(issue.number, issue.clone());
                }

                self.pending_session_launches.push(session);
            }
            TuiDataEvent::UnifiedIssues(Err(e), _) => {
                self.activity_log.push_simple(
                    "Session".into(),
                    format!("Failed to fetch issues for unified PR: {}", e),
                    LogLevel::Error,
                );
            }
        }
    }
}
