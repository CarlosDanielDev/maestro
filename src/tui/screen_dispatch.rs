use super::app;
use super::screens::{self, Screen, ScreenAction};
use crate::provider::github::types::GhIssue;
use crate::session::transition::TransitionReason;
use crossterm::event::Event;

/// Fire wizard step-entry hooks so internal transitions (Enter advances
/// inside the wizard) trigger the same fetch/launch side effects as a
/// fresh Push. Idempotent — guarded by the wizard's own `entered_*` checks.
pub(super) fn tick_wizard_step_hooks(app: &mut app::App) {
    if app.tui_mode != app::TuiMode::IssueWizard {
        return;
    }
    let (start_dep_fetch, start_review, start_improve) = match app.issue_wizard_screen.as_ref() {
        Some(s) => (
            s.entered_dependencies_step(),
            s.entered_ai_review_step(),
            s.improve_requested(),
        ),
        None => (false, false, false),
    };
    if start_dep_fetch {
        if let Some(ref mut s) = app.issue_wizard_screen {
            s.begin_dependency_fetch();
        }
        app.pending_commands
            .push(app::TuiCommand::FetchWizardDependencies);
    }
    if start_review {
        let payload = app
            .issue_wizard_screen
            .as_ref()
            .map(|s| s.payload().clone());
        if let Some(payload) = payload {
            if let Some(ref mut s) = app.issue_wizard_screen {
                s.begin_ai_review();
            }
            app.pending_commands
                .push(app::TuiCommand::LaunchAiReview(payload));
        }
    }
    if start_improve {
        let pair = app.issue_wizard_screen.as_ref().map(|s| {
            (
                s.payload().clone(),
                s.review_text().unwrap_or("").to_string(),
            )
        });
        if let Some((payload, critique)) = pair {
            if let Some(ref mut s) = app.issue_wizard_screen {
                s.mark_improve_enqueued();
            }
            app.pending_commands
                .push(app::TuiCommand::LaunchAiImprove(payload, critique));
        }
    }

    let needs_create = app
        .issue_wizard_screen
        .as_ref()
        .map(|s| {
            matches!(
                s.step(),
                crate::tui::screens::issue_wizard::IssueWizardStep::Creating
            ) && s.create_in_flight()
                && !s.create_enqueued()
        })
        .unwrap_or(false);
    if needs_create {
        let payload = app
            .issue_wizard_screen
            .as_ref()
            .map(|s| s.payload().clone());
        if let Some(payload) = payload {
            if let Some(ref mut s) = app.issue_wizard_screen {
                s.mark_create_enqueued();
            }
            app.pending_commands
                .push(app::TuiCommand::CreateIssue(payload));
        }
    }

    // Milestone wizard: AiStructuring auto-launch + Materializing creation.
    if app.tui_mode == app::TuiMode::MilestoneWizard {
        let (start_planning, start_creating) = match app.milestone_wizard_screen.as_ref() {
            Some(s) => (
                s.entered_ai_structuring_step(),
                matches!(
                    s.step(),
                    crate::tui::screens::milestone_wizard::MilestoneWizardStep::Materializing
                ) && s.materialize_progress().is_some()
                    && !s.materialize_enqueued(),
            ),
            None => (false, false),
        };
        if start_planning {
            let payload = app
                .milestone_wizard_screen
                .as_ref()
                .map(|s| s.payload().clone());
            if let Some(payload) = payload {
                if let Some(ref mut s) = app.milestone_wizard_screen {
                    s.start_planning();
                }
                app.pending_commands
                    .push(app::TuiCommand::LaunchAiPlanning(payload));
            }
        }
        if start_creating {
            let plan = app
                .milestone_wizard_screen
                .as_ref()
                .and_then(|s| s.generated_plan().cloned());
            if let Some(plan) = plan {
                if let Some(ref mut s) = app.milestone_wizard_screen {
                    s.mark_materialize_enqueued();
                }
                app.pending_commands
                    .push(app::TuiCommand::CreateMilestoneWithIssues(plan));
            }
        }
    }
}

pub(super) fn dispatch_to_active_screen_then_hook(
    app: &mut app::App,
    event: &Event,
) -> Option<ScreenAction> {
    let action = dispatch_to_active_screen(app, event);
    tick_wizard_step_hooks(app);
    action
}

pub(super) fn dispatch_to_active_screen(app: &mut app::App, event: &Event) -> Option<ScreenAction> {
    use crate::tui::navigation::InputMode;

    // Special-case the screens that don't fit the Screen trait shape (they
    // need access to App-owned data alongside their own state).
    if matches!(app.tui_mode, app::TuiMode::Prd) {
        return Some(crate::tui::screens::prd_dispatch::dispatch_input(
            app, event,
        ));
    }
    if matches!(app.tui_mode, app::TuiMode::BypassWarning) {
        return Some(crate::tui::screens::bypass_dispatch::dispatch_input(
            app, event,
        ));
    }
    if matches!(app.tui_mode, app::TuiMode::Roadmap) {
        return Some(crate::tui::screens::roadmap_dispatch::dispatch_input(
            app, event,
        ));
    }

    let screen: &mut dyn Screen = match app.tui_mode {
        app::TuiMode::Dashboard => app.home_screen.as_mut()?,
        app::TuiMode::Landing => app.landing_screen.as_mut()?,
        app::TuiMode::IssueWizard => app.issue_wizard_screen.as_mut()?,
        app::TuiMode::ProjectStats => app.project_stats_screen.as_mut()?,
        app::TuiMode::MilestoneWizard => app.milestone_wizard_screen.as_mut()?,
        app::TuiMode::IssueBrowser => app.issue_browser_screen.as_mut()?,
        app::TuiMode::MilestoneView => app.milestone_screen.as_mut()?,
        app::TuiMode::PromptInput => app.prompt_input_screen.as_mut()?,
        app::TuiMode::QueueConfirmation => app.queue_confirmation_screen.as_mut()?,
        app::TuiMode::HollowRetry => app.hollow_retry_screen.as_mut()?,
        app::TuiMode::AdaptFollowUp => app.adapt_follow_up_screen.as_mut()?,
        app::TuiMode::Sanitize => app.sanitize_screen.as_mut()?,
        app::TuiMode::Settings => app.settings_screen.as_mut()?,
        app::TuiMode::AdaptWizard => app.adapt_screen.as_mut()?,
        app::TuiMode::PrReview => app.pr_review_screen.as_mut()?,
        app::TuiMode::ReleaseNotes => app.release_notes_screen.as_mut()?,
        _ => return None,
    };
    let mode = screen.desired_input_mode().unwrap_or(InputMode::Normal);
    Some(screen.handle_input(event, mode))
}

/// Dispatch a bracketed-paste payload to the currently focused screen.
///
/// Synthesises `Event::Paste(text.to_string())` and routes it through the
/// same `Screen::handle_input` path as keys. Screens without a text field
/// fall through to `ScreenAction::None`.
pub(super) fn dispatch_paste_to_active_screen(app: &mut app::App, text: &str) {
    let event = Event::Paste(text.to_string());
    if let Some(action) = dispatch_to_active_screen(app, &event) {
        handle_screen_action(app, action);
    }
}

/// Returns milestone issues only when navigating from `MilestoneView`.
fn milestone_issues_if_applicable(app: &app::App) -> Option<Vec<GhIssue>> {
    if app.tui_mode != app::TuiMode::MilestoneView {
        return None;
    }
    app.milestone_screen.as_ref().and_then(|ms| {
        ms.selected_milestone().and_then(|entry| {
            let open_issues: Vec<GhIssue> = entry
                .issues
                .iter()
                .filter(|i| i.state == "open")
                .cloned()
                .collect();
            if open_issues.is_empty() {
                None
            } else {
                Some(open_issues)
            }
        })
    })
}

/// Process a ScreenAction returned by a screen's input handler.
pub(super) fn handle_screen_action(app: &mut app::App, action: ScreenAction) {
    match action {
        ScreenAction::None => {}
        ScreenAction::Push(mode) => {
            match mode {
                app::TuiMode::Landing => {
                    app.landing_screen
                        .get_or_insert_with(screens::LandingScreen::new);
                }
                app::TuiMode::IssueWizard => {
                    app.issue_wizard_screen
                        .get_or_insert_with(screens::IssueWizardScreen::new);
                }
                app::TuiMode::ProjectStats => {
                    app.project_stats_screen = Some(screens::ProjectStatsScreen::new());
                    app.pending_commands
                        .push(app::TuiCommand::FetchProjectStats);
                }
                app::TuiMode::MilestoneWizard => {
                    app.milestone_wizard_screen
                        .get_or_insert_with(screens::MilestoneWizardScreen::new);
                }
                app::TuiMode::IssueBrowser => {
                    let layout = app
                        .config
                        .as_ref()
                        .map(|c| c.tui.layout.clone())
                        .unwrap_or_default();
                    if let Some(issues) = milestone_issues_if_applicable(app) {
                        app.issue_browser_screen =
                            Some(screens::IssueBrowserScreen::new(issues).with_layout(layout));
                    } else {
                        // Fresh screen for "All Issues" — never reuse a
                        // milestone-scoped screen (fixes #117).
                        let mut screen =
                            screens::IssueBrowserScreen::new(vec![]).with_layout(layout);
                        screen.loading = true;
                        app.issue_browser_screen = Some(screen);
                        app.pending_commands.push(app::TuiCommand::FetchIssues);
                    }
                }
                app::TuiMode::MilestoneView if app.milestone_screen.is_none() => {
                    let mut screen = screens::MilestoneScreen::new(vec![]);
                    screen.loading = true;
                    app.milestone_screen = Some(screen);
                    app.pending_commands.push(app::TuiCommand::FetchMilestones);
                }
                app::TuiMode::Settings => {
                    if let Some(ref config) = app.config {
                        let mut screen =
                            screens::SettingsScreen::new(config.clone(), app.flags.clone());
                        if let Some(ref path) = app.config_path {
                            screen = screen.with_config_path(path.clone());
                        } else {
                            tracing::warn!(
                                "No config path resolved at boot — Settings save will surface an error"
                            );
                        }
                        app.settings_screen = Some(screen);
                    }
                }
                app::TuiMode::AdaptWizard => {
                    app.adapt_screen = Some(crate::tui::screens::adapt::AdaptScreen::new());
                }
                app::TuiMode::PrReview => {
                    app.pr_review_screen =
                        Some(crate::tui::screens::pr_review::PrReviewScreen::new());
                    app.pending_commands.push(app::TuiCommand::FetchOpenPrs);
                }
                app::TuiMode::ReleaseNotes => {
                    app.release_notes_screen = Some(crate::tui::screens::ReleaseNotesScreen::new());
                }
                app::TuiMode::PromptInput => {
                    app.prompt_input_screen = Some(app::helpers::create_prompt_input_screen(
                        &app.prompt_history,
                    ));
                }
                _ => {}
            }
            app.navigate_to(mode);
            tick_wizard_step_hooks(app);
        }
        ScreenAction::Pop => {
            match app.tui_mode {
                app::TuiMode::IssueBrowser => {
                    app.issue_browser_screen = None;
                }
                app::TuiMode::IssueWizard => {
                    app.issue_wizard_screen = None;
                }
                app::TuiMode::ProjectStats => {
                    app.project_stats_screen = None;
                }
                app::TuiMode::MilestoneWizard => {
                    app.milestone_wizard_screen = None;
                }
                app::TuiMode::MilestoneView => {
                    app.milestone_screen = None;
                }
                app::TuiMode::PromptInput => {
                    app.prompt_input_screen = None;
                }
                app::TuiMode::QueueConfirmation => {
                    app.queue_confirmation_screen = None;
                }
                app::TuiMode::HollowRetry => {
                    app.hollow_retry_screen = None;
                }
                app::TuiMode::AdaptFollowUp => {
                    app.adapt_follow_up_screen = None;
                }
                app::TuiMode::Sanitize => {
                    app.sanitize_screen = None;
                }
                app::TuiMode::Settings => {
                    app.preview_theme = None;
                    app.settings_screen = None;
                }
                app::TuiMode::AdaptWizard => {
                    app.adapt_screen = None;
                }
                app::TuiMode::PrReview => {
                    app.pr_review_screen = None;
                }
                app::TuiMode::ReleaseNotes => {
                    app.release_notes_screen = None;
                }
                _ => {}
            }
            app.navigate_back_or_dashboard();
        }
        ScreenAction::RefreshSuggestions => {
            let already_loading = app
                .home_screen
                .as_ref()
                .is_some_and(|s| s.loading_suggestions);
            if !already_loading {
                if let Some(ref mut screen) = app.home_screen {
                    screen.start_loading_suggestions();
                }
                app.pending_commands
                    .push(app::TuiCommand::FetchSuggestionData);
            }
        }
        ScreenAction::CheckForUpdate => {
            app.activity_log.push_simple(
                "UPDATE".into(),
                "Checking for updates...".into(),
                crate::tui::activity_log::LogLevel::Info,
            );
            crate::tui::background_tasks::spawn_version_check(app.data_tx.clone());
        }
        ScreenAction::UpdateConfig(config) => {
            // Detect the one field that genuinely cannot live-apply:
            // `max_concurrent` is the SessionPool's fixed capacity, set
            // at App::new time. Everything else is rebuildable.
            let max_concurrent_changed = app
                .config
                .as_ref()
                .map(|c| c.sessions.max_concurrent != config.sessions.max_concurrent)
                .unwrap_or(false);

            // 1. Visual + flags (cheap, always safe).
            crate::icon_mode::init_from_config(config.tui.ascii_icons);
            app.flags
                .set_enabled(crate::flags::Flag::TurboQuant, config.turboquant.enabled);
            let mut theme = crate::tui::theme::Theme::from_config(&config.tui.theme);
            theme.apply_capability(crate::tui::theme::ColorCapability::detect());
            app.theme = theme;
            app.preview_theme = None;
            app.show_mascot = config.tui.show_mascot;
            app.mascot_style = config.tui.mascot_style;

            // 2. Pool-level session config. Affects the next-launched
            // session; already-running sessions keep their spawn-time
            // values (Claude reads its flags once at process start).
            let new_permission_mode = config.sessions.permission_mode.clone();
            app.pool.set_permission_mode(new_permission_mode.clone());
            app.pool
                .set_allowed_tools(config.sessions.allowed_tools.clone());
            let guardrail = crate::prompts::resolve_guardrail(
                config.sessions.guardrail_prompt.as_deref(),
                &std::path::PathBuf::from("."),
            );
            app.pool.set_guardrail_prompt(guardrail);
            app.pool
                .set_knowledge_appendix(crate::adapt::knowledge::load_appendix());

            // 3. TurboQuant adapter rebuild (fork policy + pool wiring).
            let tq_adapter = if config.turboquant.enabled {
                Some(std::sync::Arc::new(
                    crate::turboquant::adapter::TurboQuantAdapter::new(config.turboquant.bit_width),
                ))
            } else {
                None
            };
            let mut fp = crate::session::fork::ForkPolicy::new(
                config.sessions.context_overflow.max_fork_depth,
            );
            if let Some(ref adapter) = tq_adapter {
                fp = fp.with_turboquant(
                    std::sync::Arc::clone(adapter),
                    config.turboquant.fork_handoff_budget,
                );
                app.pool.set_turboquant_adapter(
                    std::sync::Arc::clone(adapter),
                    config.turboquant.system_prompt_budget,
                );
            }
            app.fork_policy = Some(fp);
            app.turboquant_adapter = tq_adapter;

            // 4. Long-lived collaborators rebuilt from the new config.
            app.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(
                config.budget.per_session_usd,
                config.budget.total_usd,
                config.budget.alert_threshold_pct,
            ));
            app.model_router = Some(crate::models::ModelRouter::new(
                config.models.routing.clone(),
                config.sessions.default_model.clone(),
            ));
            app.notifications =
                crate::commands::setup::build_notification_dispatcher(&config.notifications);
            app.plugin_runner = if config.plugins.is_empty() {
                None
            } else {
                Some(crate::plugins::runner::PluginRunner::new(
                    config.plugins.clone(),
                    crate::commands::setup::DEFAULT_PLUGIN_TIMEOUT_SECS,
                ))
            };
            app.prompt_history
                .set_max_entries(config.sessions.max_prompt_history);

            // 5. Bypass flag follows permission_mode.
            let should_bypass = new_permission_mode == "bypassPermissions";
            if should_bypass && !app.bypass_active {
                app.confirm_bypass_activation("settings");
            } else if !should_bypass && app.bypass_active {
                app.deactivate_bypass("settings");
            }

            // 6. Activity-log feedback. Tells the user what happened and
            // — critically — calls out the one field that needs restart.
            app.activity_log.push_simple(
                "SETTINGS".into(),
                "Settings saved and applied (theme, sessions, budget, notifications, plugins)."
                    .into(),
                crate::tui::activity_log::LogLevel::Info,
            );
            if max_concurrent_changed {
                app.activity_log.push_simple(
                    "SETTINGS".into(),
                    format!(
                        "max_concurrent changed to {} — RESTART required (pool capacity is fixed at startup).",
                        config.sessions.max_concurrent
                    ),
                    crate::tui::activity_log::LogLevel::Warn,
                );
            }

            app.config = Some(*config);
        }
        ScreenAction::PreviewTheme(theme_config) => {
            if let Some(tc) = theme_config {
                let mut theme = crate::tui::theme::Theme::from_config(&tc);
                theme.apply_capability(crate::tui::theme::ColorCapability::detect());
                app.preview_theme = Some(theme);
            } else {
                app.preview_theme = None;
            }
        }
        ScreenAction::LaunchUnifiedSession(config) => {
            app.pending_commands
                .push(app::TuiCommand::LaunchUnifiedSession(config));
            app.nav_stack.clear();
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchSession(config) => {
            app.pending_commands
                .push(app::TuiCommand::LaunchSession(config));
            app.nav_stack.clear();
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchSessions(configs) => {
            app.pending_commands
                .push(app::TuiCommand::LaunchSessions(configs));
            app.nav_stack.clear();
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchPromptSession(config) => {
            app.prompt_input_screen = None;
            app.adapt_follow_up_screen = None;
            app.pending_commands
                .push(app::TuiCommand::LaunchPromptSession(config));
            app.nav_stack.clear();
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchConflictFix(config) => {
            app.spawn_conflict_fix_session(&config);
            app.completion_summary = None;
            app.nav_stack.clear();
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::RetryHollow(session_id) => {
            // Queue a retry for the hollow session
            if let Some(managed) = app.pool.get_active_mut(session_id) {
                let policy = app
                    .config
                    .as_ref()
                    .map(|c| crate::session::retry::RetryPolicy::from_config(&c.sessions));
                if let Some(policy) = policy {
                    let progress = app.progress_tracker.get(&session_id).cloned();
                    let retry = policy.prepare_retry(&managed.session, progress.as_ref(), None);
                    let label = crate::tui::app::helpers::session_label(&managed.session);
                    let _ = managed.session.transition_to(
                        crate::session::types::SessionStatus::Retrying,
                        TransitionReason::RetryTriggered,
                    );
                    app.activity_log.push_simple(
                        label,
                        "Manual retry (hollow completion)".into(),
                        crate::tui::activity_log::LogLevel::Warn,
                    );
                    app.pending_session_launches.push(retry);
                }
            }
            app.hollow_retry_screen = None;
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::FetchPrDetail(pr_number) => {
            let pr = app
                .pr_review_screen
                .as_ref()
                .and_then(|s| s.find_pr(pr_number));
            if let (Some(pr), Some(ref mut screen)) = (pr, app.pr_review_screen.as_mut()) {
                screen.set_pr_detail(pr);
            }
        }
        ScreenAction::SubmitPrReview {
            pr_number,
            event,
            body,
        } => {
            app.pending_commands.push(app::TuiCommand::SubmitPrReview {
                pr_number,
                event,
                body,
            });
        }
        ScreenAction::OpenIssueWizardForMilestone {
            milestone,
            suggested_blocked_by,
        } => {
            // Reuse an existing wizard if present, otherwise spin one up.
            // Pre-fill milestone + suggested Blocked By so the user can
            // accept/override on the Dependencies step.
            let mut wizard = app.issue_wizard_screen.take().unwrap_or_default();
            {
                let payload = wizard.payload_mut();
                payload.milestone = Some(milestone);
                payload.blocked_by = suggested_blocked_by;
            }
            app.issue_wizard_screen = Some(wizard);
            app.navigate_to(app::TuiMode::IssueWizard);
        }
        ScreenAction::StartAdaptPipeline(config) => {
            if let Some(ref mut screen) = app.adapt_screen {
                use crate::tui::screens::adapt::types::AdaptStep;
                match screen.step {
                    AdaptStep::Configure | AdaptStep::Scanning => {
                        screen.step = AdaptStep::Scanning;
                        app.pending_commands
                            .push(app::TuiCommand::RunAdaptScan(config));
                    }
                    AdaptStep::Analyzing => {
                        if let Some(profile) = screen.results.profile.clone() {
                            app.pending_commands
                                .push(app::TuiCommand::RunAdaptAnalyze(config, profile));
                        }
                    }
                    AdaptStep::Consolidating => {
                        if let (Some(profile), Some(report)) = (
                            screen.results.profile.clone(),
                            screen.results.report.clone(),
                        ) {
                            app.pending_commands
                                .push(app::TuiCommand::RunAdaptConsolidate(
                                    config, profile, report,
                                ));
                        }
                    }
                    AdaptStep::Planning => {
                        if let (Some(profile), Some(report)) = (
                            screen.results.profile.clone(),
                            screen.results.report.clone(),
                        ) {
                            let prd = screen.results.prd_content.clone();
                            app.pending_commands
                                .push(app::TuiCommand::RunAdaptPlan(config, profile, report, prd));
                        }
                    }
                    AdaptStep::Scaffolding => {
                        if let (Some(profile), Some(report), Some(plan)) = (
                            screen.results.profile.clone(),
                            screen.results.report.clone(),
                            screen.results.plan.clone(),
                        ) {
                            app.pending_commands.push(app::TuiCommand::RunAdaptScaffold(
                                config, profile, report, plan,
                            ));
                        }
                    }
                    AdaptStep::Materializing => {
                        if let (Some(plan), Some(report)) =
                            (screen.results.plan.clone(), screen.results.report.clone())
                        {
                            app.pending_commands
                                .push(app::TuiCommand::RunAdaptMaterialize(plan, report));
                        }
                    }
                    _ => {}
                }
            }
        }
        ScreenAction::LaunchQueue(configs) => {
            use crate::work::dependencies::DependencyGraph;
            use crate::work::executor::QueueExecutor;
            use crate::work::queue::WorkQueue;
            use crate::work::types::WorkItem;

            // Build a WorkQueue from the session configs for the executor
            let issue_numbers: Vec<u64> = configs.iter().filter_map(|c| c.issue_number).collect();

            // Build a minimal dependency graph (items are already validated by QueueConfirmation)
            let items: Vec<WorkItem> = configs
                .iter()
                .filter_map(|c| {
                    c.issue_number.map(|n| {
                        WorkItem::from_issue(crate::provider::github::types::GhIssue {
                            number: n,
                            title: c.title.clone(),
                            body: String::new(),
                            labels: vec![],
                            state: "open".to_string(),
                            html_url: String::new(),
                            milestone: None,
                            assignees: vec![],
                        })
                    })
                })
                .collect();
            let graph = DependencyGraph::build(&items);

            if let Ok(queue) = WorkQueue::validate_selection(&issue_numbers, &graph) {
                let executor = QueueExecutor::new(&queue);
                app.queue_launch_configs = Some(configs);
                app.queue_executor = Some(executor);
                app.advance_queue_and_launch();
                app.completion_summary_dismissed = false;
                app.tui_mode = app::TuiMode::QueueExecution;
            } else {
                // Fallback: launch all at once if queue validation fails
                app.pending_commands
                    .push(app::TuiCommand::LaunchSessions(configs));
                app.tui_mode = app::TuiMode::Overview;
            }
        }
    }
}
