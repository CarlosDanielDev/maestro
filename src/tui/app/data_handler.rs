use super::App;
use super::types::TuiDataEvent;
use crate::prd::model::{MergeReport, Prd};
use crate::prd::sync::PrdSyncResult;
use crate::session::types::Session;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::milestone::MilestoneEntry;

/// Render a `MergeReport` as a one-line activity-log suffix, prefixed
/// with the source identifier. Returns `None` when nothing was added so
/// the caller can omit the noise.
pub(crate) fn format_merge_summary(
    report: &MergeReport,
    source_id: &str,
    source_label: &str,
) -> Option<String> {
    if report.is_no_op() {
        return None;
    }
    let mut parts = Vec::new();
    if report.filled_vision {
        parts.push("vision".to_string());
    }
    if report.added_goals > 0 {
        parts.push(format!("{} goal(s)", report.added_goals));
    }
    if report.added_non_goals > 0 {
        parts.push(format!("{} non-goal(s)", report.added_non_goals));
    }
    if report.added_stakeholders > 0 {
        parts.push(format!("{} stakeholder(s)", report.added_stakeholders));
    }
    Some(format!(
        "ingested from {source_id} ({source_label}): {}",
        parts.join(", ")
    ))
}

fn fold_ingest_into_prd(prd_slot: &mut Option<Prd>, sync: &PrdSyncResult) -> Option<String> {
    let report = sync.ingested.as_ref()?;
    let prd = prd_slot.as_mut()?;
    let merge = prd.merge_ingested(&report.ingested);
    format_merge_summary(
        &merge,
        &format!("#{}", report.issue_number),
        report.source_label,
    )
}

impl App {
    /// Resolve model and mode from config and issue labels.
    fn resolve_model_and_mode(&self, labels: &[String]) -> (String, String) {
        let model = self.session_config.default_model.clone();
        let default_mode = self.session_config.default_mode.clone();
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
                if let Some(ref mut screen) = self.screen_state.issue_browser_screen {
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
                if let Some(ref mut screen) = self.screen_state.issue_browser_screen {
                    screen.loading = false;
                }
            }
            TuiDataEvent::Milestones(Ok(entries)) => {
                if matches!(self.tui_mode, crate::tui::app::TuiMode::MilestoneHealth)
                    && let Some(ref mut screen) = self.screen_state.milestone_health_screen
                {
                    let milestones: Vec<_> = entries.iter().map(|(m, _)| m.clone()).collect();
                    if let Some(cmd) = screen.apply_milestones_loaded(Ok(milestones)) {
                        self.pending_commands.push(cmd);
                    }
                }
                if let Some(ref mut screen) = self.screen_state.milestone_screen {
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
                if matches!(self.tui_mode, crate::tui::app::TuiMode::MilestoneHealth)
                    && let Some(ref mut screen) = self.screen_state.milestone_health_screen
                {
                    let _ = screen
                        .apply_milestones_loaded(Err(anyhow::anyhow!("milestones fetch failed")));
                }
                if let Some(ref mut screen) = self.screen_state.milestone_screen {
                    screen.loading = false;
                }
            }
            TuiDataEvent::Issue(Ok(gh_issue), custom_prompt) => {
                let (model, issue_mode) = self.resolve_model_and_mode(&gh_issue.labels);
                let prompt = self.build_issue_prompt_with_custom(&gh_issue, &custom_prompt);
                let issue_number = gh_issue.number;
                let mut session = Session::new(prompt, model, issue_mode, Some(issue_number), None);
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
                if let Some(ref mut screen) = self.screen_state.home_screen {
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
            TuiDataEvent::VersionCheckResult(None) => {
                self.activity_log.push_simple(
                    "UPDATE".into(),
                    "Already on the latest version.".into(),
                    LogLevel::Info,
                );
            }
            TuiDataEvent::UpgradeResult(Ok(backup_path)) => {
                if let crate::updater::UpgradeState::Downloading { version } = &self.upgrade_state {
                    self.upgrade_state = crate::updater::UpgradeState::ReadyToRestart {
                        version: version.clone(),
                        backup_path,
                    };
                    self.activity_log.push_simple(
                        "UPDATE".into(),
                        "Update successful — please restart maestro to apply changes.".into(),
                        LogLevel::Info,
                    );
                }
            }
            TuiDataEvent::UpgradeResult(Err(msg)) => {
                self.upgrade_state = crate::updater::UpgradeState::Failed(msg);
            }
            TuiDataEvent::AdaptScanResult(result) => {
                if let Some(ref mut screen) = self.screen_state.adapt_screen {
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
                if let Some(ref mut screen) = self.screen_state.adapt_screen {
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
                if let Some(ref mut screen) = self.screen_state.adapt_screen {
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
                if let Some(ref mut screen) = self.screen_state.adapt_screen {
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
            TuiDataEvent::AdaptScaffoldResult(result) => {
                if let Some(ref mut screen) = self.screen_state.adapt_screen {
                    if screen.is_cancelled() {
                        return;
                    }
                    match result {
                        Ok(scaffold_result) => {
                            if let Some(cmd) = screen.complete_scaffold(scaffold_result) {
                                self.pending_commands.push(cmd);
                            }
                        }
                        Err(e) => {
                            screen.set_error(
                                crate::tui::screens::adapt::AdaptStep::Scaffolding,
                                format!("{}", e),
                            );
                        }
                    }
                }
            }
            TuiDataEvent::PullRequests(Ok(prs)) => {
                if let Some(ref mut screen) = self.screen_state.pr_review_screen {
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
                if let Some(ref mut screen) = self.screen_state.pr_review_screen {
                    screen.set_loading_error(&format!("{}", e));
                }
            }
            TuiDataEvent::PrReviewSubmitted(Ok(())) => {
                self.activity_log.push_simple(
                    "PR Review".into(),
                    "Review submitted successfully".into(),
                    LogLevel::Info,
                );
                if let Some(ref mut screen) = self.screen_state.pr_review_screen {
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
                if let Some(ref mut screen) = self.screen_state.pr_review_screen {
                    screen.set_error(&format!("{}", e));
                }
            }
            TuiDataEvent::AdaptMaterializeResult(result) => {
                if let Some(ref mut screen) = self.screen_state.adapt_screen {
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
                let mut session =
                    Session::new(combined_prompt, model, issue_mode, primary_issue, None);
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
            TuiDataEvent::IssueCreated(result) => {
                if let Some(ref mut screen) = self.screen_state.issue_wizard_screen {
                    screen.finish_create(result);
                }
            }
            TuiDataEvent::IssueAlreadyExists {
                number,
                state,
                title,
            } => {
                if let Some(ref mut screen) = self.screen_state.issue_wizard_screen {
                    screen.finish_create_already_exists(number, state, title);
                }
            }
            TuiDataEvent::ProjectStats(data) => {
                if let Some(ref mut screen) = self.screen_state.project_stats_screen {
                    screen.set_data(data);
                }
            }
            TuiDataEvent::AiPlanningResult(result) => {
                if let Some(ref mut screen) = self.screen_state.milestone_wizard_screen {
                    screen.apply_planning_result(result);
                }
            }
            TuiDataEvent::WizardDependencyIssues(result) => match result {
                Ok(issues) => {
                    if let Some(ref mut screen) = self.screen_state.issue_wizard_screen {
                        screen.apply_dep_issues(issues);
                    }
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        "Wizard".into(),
                        format!("Failed to fetch issues for Dependencies step: {}", e),
                        LogLevel::Error,
                    );
                    if let Some(ref mut screen) = self.screen_state.issue_wizard_screen {
                        screen.apply_dep_issues(Vec::new());
                    }
                }
            },
            TuiDataEvent::AiReviewResult(result) => {
                if let Some(ref mut screen) = self.screen_state.issue_wizard_screen {
                    screen.apply_ai_review(result);
                }
            }
            TuiDataEvent::AiImproveResult(result) => {
                if let Some(ref mut screen) = self.screen_state.issue_wizard_screen {
                    screen.apply_improve_result(result);
                }
            }
            TuiDataEvent::MilestonePlanCreated(result) => {
                if let Some(ref mut screen) = self.screen_state.milestone_wizard_screen {
                    screen.finish_materialization(result);
                }
            }
            TuiDataEvent::PrdSyncResult(result) => match result {
                Ok(sync) => {
                    let milestone_count = sync.timeline.len();
                    let candidate_count = sync.candidates.len();
                    self.prd_candidates = sync.candidates.clone();
                    // Pre-parse so the explore renderer doesn't re-parse
                    // markdown bodies on every frame.
                    self.prd_candidate_parsed = self
                        .prd_candidates
                        .iter()
                        .map(|c| crate::prd::ingest::parse_markdown(&c.body))
                        .collect();
                    let ingest_summary = fold_ingest_into_prd(&mut self.prd, &sync);
                    if let Some(prd) = self.prd.as_mut() {
                        prd.current_state = sync.current_state.clone();
                        prd.timeline = sync.timeline.clone();
                        if let Some(s) = self.screen_state.prd_screen.as_mut() {
                            s.dirty = true;
                            s.sync_status = crate::tui::screens::prd::PrdSyncStatus::SyncedAt(
                                std::time::Instant::now(),
                            );
                        }
                        let suffix = ingest_summary
                            .map(|s| format!(" • {s}"))
                            .unwrap_or_default();
                        let explore_hint = if candidate_count > 1 {
                            format!(" • {candidate_count} sources found, press [o] to explore")
                        } else {
                            String::new()
                        };
                        self.activity_log.push_simple(
                            "PRD".into(),
                            format!(
                                "PRD synced — {milestone_count} milestone(s){suffix}{explore_hint}"
                            ),
                            LogLevel::Info,
                        );
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if let Some(s) = self.screen_state.prd_screen.as_mut() {
                        s.sync_status = crate::tui::screens::prd::PrdSyncStatus::Failed {
                            at: std::time::Instant::now(),
                            message: msg.clone(),
                        };
                    }
                    self.activity_log.push_simple(
                        "PRD".into(),
                        format!("PRD sync failed: {msg}"),
                        LogLevel::Error,
                    );
                }
            },
            TuiDataEvent::RoadmapResult(result) => match result {
                Ok(entries) => {
                    if let Some(s) = self.screen_state.roadmap_screen.as_mut() {
                        s.set_entries(entries);
                    }
                }
                Err(e) => self.activity_log.push_simple(
                    "Roadmap".into(),
                    format!("Roadmap fetch failed: {e}"),
                    LogLevel::Error,
                ),
            },
            TuiDataEvent::ReviewCycleResult { pr_number, result } => match result {
                Ok(report) => {
                    let count = report.concerns.len();
                    self.pending_review_report = Some(report);
                    self.concerns_cursor = 0;
                    self.activity_log.push_simple(
                        "Review".into(),
                        format!(
                            "Review for PR #{pr_number}: {count} concern(s) — press [C] to view"
                        ),
                        LogLevel::Info,
                    );
                    // Bypass auto-disable hook (#328 AC). Auto-disable
                    // also fires when concerns exist but bypass is on so
                    // accepted-and-applied → cycle complete.
                    if self.bypass_active && count == 0 {
                        self.deactivate_bypass("review-cycle-complete");
                    }
                }
                Err(e) => self.activity_log.push_simple(
                    "Review".into(),
                    format!("Auto-review failed for PR #{pr_number}: {e}"),
                    LogLevel::Error,
                ),
            },
            TuiDataEvent::MilestoneHealthIssuesFetched(result) => {
                if let Some(ref mut screen) = self.screen_state.milestone_health_screen {
                    let cmd = screen.apply_issues_fetched(result);
                    if let Some(cmd) = cmd {
                        self.pending_commands.push(cmd);
                    }
                }
            }
            TuiDataEvent::MilestoneHealthPatched(result) => {
                if let Some(ref mut screen) = self.screen_state.milestone_health_screen {
                    screen.apply_patch_result(result);
                }
            }
        }
    }
}
