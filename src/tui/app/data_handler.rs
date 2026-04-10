use super::App;
use super::types::TuiDataEvent;
use crate::session::types::Session;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::milestone::MilestoneEntry;

impl App {
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
                let issue_mode =
                    crate::modes::mode_from_labels(&gh_issue.labels).unwrap_or(default_mode);
                let issue_number = gh_issue.number;
                let base_prompt = self
                    .config
                    .as_ref()
                    .map(|c| crate::prompts::PromptBuilder::build_issue_prompt(&gh_issue, c))
                    .unwrap_or_else(|| gh_issue.unattended_prompt());
                let prompt = match custom_prompt {
                    Some(ref cp) if !cp.trim().is_empty() => {
                        format!(
                            "{}\n\n## Additional Instructions\n\n{}",
                            base_prompt,
                            cp.trim()
                        )
                    }
                    _ => base_prompt,
                };
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
                let suggestions = crate::tui::screens::home::Suggestion::build_suggestions(
                    payload.ready_issue_count,
                    payload.failed_issue_count,
                    &payload.milestones,
                    active,
                );
                if let Some(ref mut screen) = self.home_screen {
                    screen.set_suggestions(suggestions);
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
        }
    }
}
