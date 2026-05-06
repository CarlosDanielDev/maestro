use super::App;
use super::types::TuiDataEvent;
use crate::prd::model::{MergeReport, Prd};
use crate::prd::sync::PrdSyncResult;
use crate::session::types::{Session, SessionModeConfig};
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
    fn resolve_model_and_mode(
        &self,
        labels: &[String],
        agent_id: Option<&str>,
    ) -> (String, String, Option<SessionModeConfig>) {
        let model = agent_id
            .and_then(|id| {
                self.config
                    .as_ref()
                    .and_then(|c| c.resolve_agent(Some(id)).ok())
                    .and_then(|resolved| resolved.config.model)
            })
            .unwrap_or_else(|| self.session_config.default_model.clone());
        let default_mode = self.session_config.default_mode.clone();
        let (mode, mode_config) =
            crate::modes::resolve_mode_for_labels(labels, &default_mode, self.config.as_ref());
        (model, mode, mode_config)
    }

    /// Build a prompt from issue + optional custom instructions.
    fn build_issue_prompt_with_custom(
        &self,
        gh_issue: &crate::provider::types::Issue,
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
            TuiDataEvent::Issue(Ok(gh_issue), custom_prompt, agent_id) => {
                let agent_id = agent_id.unwrap_or_else(|| self.selected_agent_id());
                let (model, issue_mode, mode_config) =
                    self.resolve_model_and_mode(&gh_issue.labels, Some(&agent_id));
                let prompt = self.build_issue_prompt_with_custom(&gh_issue, &custom_prompt);
                let issue_number = gh_issue.number;
                let mut session = Session::new(prompt, model, issue_mode, Some(issue_number), None)
                    .with_mode_config(mode_config)
                    .with_agent_id(Some(agent_id));
                session.issue_title = Some(gh_issue.title.clone());
                self.state.issue_cache.insert(issue_number, gh_issue);
                self.pending_session_launches.push(session);
            }
            TuiDataEvent::Issue(Err(e), _, _) => {
                self.activity_log.push_simple(
                    "Session".into(),
                    format!("Failed to fetch issue: {}", e),
                    LogLevel::Error,
                );
            }
            TuiDataEvent::SuggestionData(Ok(payload)) => {
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
            TuiDataEvent::SuggestionData(Err(e)) => {
                self.activity_log.push_simple(
                    "GitHub".into(),
                    format!("Failed to fetch suggestion data: {}", e),
                    LogLevel::Error,
                );
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
            TuiDataEvent::UnifiedIssues(Ok(gh_issues), custom_prompt, agent_id) => {
                let agent_id = agent_id.unwrap_or_else(|| self.selected_agent_id());
                let first_labels = gh_issues
                    .first()
                    .map(|i| i.labels.as_slice())
                    .unwrap_or(&[]);
                let (model, issue_mode, mode_config) =
                    self.resolve_model_and_mode(first_labels, Some(&agent_id));
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
                    Session::new(combined_prompt, model, issue_mode, primary_issue, None)
                        .with_mode_config(mode_config)
                        .with_agent_id(Some(agent_id));
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
            TuiDataEvent::UnifiedIssues(Err(e), _, _) => {
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
            TuiDataEvent::ProjectStats(Ok(data)) => {
                if let Some(ref mut screen) = self.screen_state.project_stats_screen {
                    screen.set_data(data);
                }
            }
            TuiDataEvent::ProjectStats(Err(e)) => {
                self.activity_log.push_simple(
                    "GitHub".into(),
                    format!("Failed to fetch project stats: {}", e),
                    LogLevel::Error,
                );
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
                        s.is_loading = false;
                        s.set_entries(entries);
                    }
                }
                Err(e) => {
                    if let Some(s) = self.screen_state.roadmap_screen.as_mut() {
                        s.is_loading = false;
                    }
                    self.activity_log.push_simple(
                        "Roadmap".into(),
                        format!("Roadmap fetch failed: {e}"),
                        LogLevel::Error,
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::types::Issue;

    fn issue_with_labels(labels: &[&str]) -> Issue {
        Issue {
            number: 402,
            title: "Wire mode resolver".to_string(),
            body: String::new(),
            labels: labels.iter().map(|label| label.to_string()).collect(),
            state: "open".to_string(),
            html_url: "https://github.com/owner/repo/issues/402".to_string(),
            milestone: None,
            assignees: Vec::new(),
        }
    }

    #[test]
    fn issue_event_applies_labeled_vibe_mode_config() {
        let mut app = crate::tui::make_test_app("issue-402-vibe-mode");

        app.handle_data_event(TuiDataEvent::Issue(
            Ok(issue_with_labels(&["maestro:mode:vibe"])),
            None,
            None,
        ));

        let session = &app.pending_session_launches[0];
        assert_eq!(session.mode, "vibe");
        assert!(
            session
                .mode_config
                .as_ref()
                .is_some_and(|mode| mode.system_prompt.contains("vibe mode"))
        );
    }

    #[test]
    fn issue_event_uses_default_mode_when_label_absent() {
        let mut app = crate::tui::make_test_app("issue-402-default-mode");

        app.handle_data_event(TuiDataEvent::Issue(Ok(issue_with_labels(&[])), None, None));

        let session = &app.pending_session_launches[0];
        assert_eq!(session.mode, "orchestrator");
        assert!(session.mode_config.is_some());
    }

    #[test]
    fn issue_event_persists_selected_agent_id() {
        let mut app = crate::tui::make_test_app("issue-402-agent-id");

        app.handle_data_event(TuiDataEvent::Issue(
            Ok(issue_with_labels(&[])),
            None,
            Some("codex".to_string()),
        ));

        assert_eq!(
            app.pending_session_launches[0].agent_id.as_deref(),
            Some("codex")
        );
    }
}

#[cfg(test)]
mod adapt_chaining_migrated_tests {
    use crate::tui::app::*;

    fn make_app() -> crate::tui::app::App {
        crate::tui::make_test_app("maestro-tui-app-test")
    }

    mod adapt_chaining {
        use super::*;
        use crate::adapt::types::*;
        use crate::tui::screens::adapt::{AdaptScreen, AdaptStep};

        fn make_profile() -> ProjectProfile {
            ProjectProfile {
                name: "test".into(),
                root: std::path::PathBuf::from("/tmp"),
                language: ProjectLanguage::Rust,
                manifests: vec![],
                config_files: vec![],
                entry_points: vec![],
                source_stats: SourceStats {
                    total_files: 10,
                    total_lines: 500,
                    by_extension: vec![],
                },
                test_infra: TestInfraInfo {
                    has_tests: true,
                    framework: None,
                    test_directories: vec![],
                    test_file_count: 0,
                },
                ci: CiInfo {
                    provider: None,
                    config_files: vec![],
                },
                git: GitInfo {
                    is_git_repo: true,
                    default_branch: Some("main".into()),
                    remote_url: None,
                    commit_count: 10,
                    recent_contributors: vec![],
                },
                dependencies: DependencySummary::default(),
                directory_tree: String::new(),
                has_maestro_config: false,
                has_workflow_docs: false,
            }
        }

        fn make_report() -> AdaptReport {
            AdaptReport {
                summary: "Test".into(),
                modules: vec![],
                tech_debt_items: vec![],
            }
        }

        fn make_plan() -> AdaptPlan {
            AdaptPlan {
                milestones: vec![],
                maestro_toml_patch: None,
                workflow_guide: None,
            }
        }

        fn make_materialize_result() -> MaterializeResult {
            MaterializeResult {
                milestones_created: vec![],
                issues_created: vec![],
                issues_skipped: vec![],
                tech_debt_issue: None,
                dry_run: false,
            }
        }

        fn app_with_adapt_screen() -> App {
            let mut app = make_app();
            app.screen_state.adapt_screen = Some(AdaptScreen::new());
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Scanning;
            app
        }

        #[test]
        fn scan_ok_chains_to_analyze() {
            let mut app = app_with_adapt_screen();
            app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Analyzing);
            assert!(screen.results.profile.is_some());
            assert_eq!(app.pending_commands.len(), 1);
            assert!(matches!(
                app.pending_commands[0],
                TuiCommand::RunAdaptAnalyze(_, _)
            ));
        }

        #[test]
        fn scan_ok_with_scan_only_completes() {
            let mut app = app_with_adapt_screen();
            app.screen_state
                .adapt_screen
                .as_mut()
                .unwrap()
                .config
                .scan_only = true;

            app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Complete);
            assert!(app.pending_commands.is_empty());
        }

        #[test]
        fn scan_err_sets_failed() {
            let mut app = app_with_adapt_screen();
            app.handle_data_event(TuiDataEvent::AdaptScanResult(Err(anyhow::anyhow!(
                "scan failed"
            ))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Failed);
            assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Scanning);
        }

        #[test]
        fn analyze_ok_chains_to_consolidate() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Analyzing;
            app.screen_state
                .adapt_screen
                .as_mut()
                .unwrap()
                .set_scan_result(make_profile());

            app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Ok(make_report())));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Consolidating);
            assert!(screen.results.report.is_some());
            assert_eq!(app.pending_commands.len(), 1);
            assert!(matches!(
                app.pending_commands[0],
                TuiCommand::RunAdaptConsolidate(_, _, _)
            ));
        }

        #[test]
        fn analyze_ok_with_no_issues_completes() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Analyzing;
            app.screen_state
                .adapt_screen
                .as_mut()
                .unwrap()
                .config
                .no_issues = true;

            app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Ok(make_report())));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Complete);
            assert!(app.pending_commands.is_empty());
        }

        #[test]
        fn analyze_err_sets_failed() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Analyzing;

            app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Err(anyhow::anyhow!(
                "analyze failed"
            ))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Failed);
            assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Analyzing);
        }

        #[test]
        fn plan_ok_chains_to_materialize() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Planning;
            app.screen_state
                .adapt_screen
                .as_mut()
                .unwrap()
                .set_scan_result(make_profile());
            app.screen_state
                .adapt_screen
                .as_mut()
                .unwrap()
                .set_analyze_result(make_report());

            app.handle_data_event(TuiDataEvent::AdaptPlanResult(Ok(make_plan())));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Scaffolding);
            assert!(screen.results.plan.is_some());
            assert_eq!(app.pending_commands.len(), 1);
            assert!(matches!(
                app.pending_commands[0],
                TuiCommand::RunAdaptScaffold(_, _, _, _)
            ));
        }

        #[test]
        fn plan_ok_with_dry_run_completes() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Planning;
            app.screen_state
                .adapt_screen
                .as_mut()
                .unwrap()
                .config
                .dry_run = true;

            app.handle_data_event(TuiDataEvent::AdaptPlanResult(Ok(make_plan())));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Complete);
            assert!(app.pending_commands.is_empty());
        }

        #[test]
        fn plan_err_sets_failed() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Planning;

            app.handle_data_event(TuiDataEvent::AdaptPlanResult(Err(anyhow::anyhow!(
                "plan failed"
            ))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Failed);
            assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Planning);
        }

        #[test]
        fn materialize_ok_completes() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Materializing;

            app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Ok(
                make_materialize_result(),
            )));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Complete);
            assert!(screen.results.materialize.is_some());
        }

        #[test]
        fn materialize_err_sets_failed() {
            let mut app = app_with_adapt_screen();
            app.screen_state.adapt_screen.as_mut().unwrap().step = AdaptStep::Materializing;

            app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Err(anyhow::anyhow!(
                "materialize failed"
            ))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert_eq!(screen.step, AdaptStep::Failed);
            assert_eq!(
                screen.error.as_ref().unwrap().phase,
                AdaptStep::Materializing
            );
        }

        #[test]
        fn cancelled_screen_ignores_scan_result() {
            let mut app = app_with_adapt_screen();
            let screen = app.screen_state.adapt_screen.as_mut().unwrap();
            screen.cancelled = true;
            screen.results = crate::tui::screens::adapt::AdaptResults::default();

            app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));

            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            // Step stays at Scanning (not transitioned)
            assert_eq!(screen.step, AdaptStep::Scanning);
            assert!(screen.results.profile.is_none());
            assert!(app.pending_commands.is_empty());
        }

        #[test]
        fn full_pipeline_happy_path() {
            let mut app = app_with_adapt_screen();

            // Phase 1: Scan
            app.handle_data_event(TuiDataEvent::AdaptScanResult(Ok(Box::new(make_profile()))));
            assert_eq!(
                app.screen_state.adapt_screen.as_ref().unwrap().step,
                AdaptStep::Analyzing
            );

            // Phase 2: Analyze
            let cmd = app.pending_commands.pop().unwrap();
            assert!(matches!(cmd, TuiCommand::RunAdaptAnalyze(_, _)));
            app.handle_data_event(TuiDataEvent::AdaptAnalyzeResult(Ok(make_report())));
            assert_eq!(
                app.screen_state.adapt_screen.as_ref().unwrap().step,
                AdaptStep::Consolidating
            );

            // Phase 2.5: Consolidate (PRD)
            let cmd = app.pending_commands.pop().unwrap();
            assert!(matches!(cmd, TuiCommand::RunAdaptConsolidate(_, _, _)));
            app.handle_data_event(TuiDataEvent::AdaptConsolidateResult(Ok(
                "# PRD: Test".to_string()
            )));
            assert_eq!(
                app.screen_state.adapt_screen.as_ref().unwrap().step,
                AdaptStep::Planning
            );

            // Phase 3: Plan
            let cmd = app.pending_commands.pop().unwrap();
            assert!(matches!(cmd, TuiCommand::RunAdaptPlan(_, _, _, _)));
            app.handle_data_event(TuiDataEvent::AdaptPlanResult(Ok(make_plan())));
            assert_eq!(
                app.screen_state.adapt_screen.as_ref().unwrap().step,
                AdaptStep::Scaffolding
            );

            // Phase 3.5: Scaffold
            let cmd = app.pending_commands.pop().unwrap();
            assert!(matches!(cmd, TuiCommand::RunAdaptScaffold(_, _, _, _)));
            use crate::adapt::types::ScaffoldResult;
            app.handle_data_event(TuiDataEvent::AdaptScaffoldResult(Ok(ScaffoldResult {
                files: vec![],
                created_count: 0,
                skipped_count: 0,
            })));
            assert_eq!(
                app.screen_state.adapt_screen.as_ref().unwrap().step,
                AdaptStep::Materializing
            );

            // Phase 4: Materialize
            let cmd = app.pending_commands.pop().unwrap();
            assert!(matches!(cmd, TuiCommand::RunAdaptMaterialize(_, _)));
            app.handle_data_event(TuiDataEvent::AdaptMaterializeResult(Ok(
                make_materialize_result(),
            )));
            assert_eq!(
                app.screen_state.adapt_screen.as_ref().unwrap().step,
                AdaptStep::Complete
            );

            // All results stored
            let screen = app.screen_state.adapt_screen.as_ref().unwrap();
            assert!(screen.results.profile.is_some());
            assert!(screen.results.report.is_some());
            assert!(screen.results.plan.is_some());
            assert!(screen.results.materialize.is_some());
        }

        #[test]
        fn home_screen_a_key_navigates_to_adapt_wizard() {
            use crate::tui::screens::home::{HomeScreen, ProjectInfo};
            use crate::tui::screens::test_helpers::key_event;
            use crate::tui::screens::{Screen, ScreenAction};
            use crossterm::event::KeyCode;

            let mut screen = HomeScreen::new(
                ProjectInfo {
                    repo: "owner/repo".to_string(),
                    branch: "main".to_string(),
                    username: None,
                },
                vec![],
                vec![],
            );

            let action = screen.handle_input(
                &key_event(KeyCode::Char('a')),
                crate::tui::navigation::InputMode::Normal,
            );
            assert_eq!(action, ScreenAction::Push(TuiMode::AdaptWizard));
        }
    }
}
