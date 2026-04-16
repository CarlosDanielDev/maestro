use super::app;
use super::screens::{self, Screen, ScreenAction};
use crate::github::types::GhIssue;
use crate::session::transition::TransitionReason;
use crossterm::event::Event;

pub(super) fn dispatch_to_active_screen(app: &mut app::App, event: &Event) -> Option<ScreenAction> {
    use crate::tui::navigation::InputMode;

    let screen: &mut dyn Screen = match app.tui_mode {
        app::TuiMode::Dashboard => app.home_screen.as_mut()?,
        app::TuiMode::IssueBrowser => app.issue_browser_screen.as_mut()?,
        app::TuiMode::MilestoneView => app.milestone_screen.as_mut()?,
        app::TuiMode::PromptInput => app.prompt_input_screen.as_mut()?,
        app::TuiMode::QueueConfirmation => app.queue_confirmation_screen.as_mut()?,
        app::TuiMode::HollowRetry => app.hollow_retry_screen.as_mut()?,
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
                        // Try to find the config file path for save
                        for candidate in &["maestro.toml", ".maestro/config.toml"] {
                            let path = std::path::PathBuf::from(candidate);
                            if path.exists() {
                                screen = screen.with_config_path(path);
                                break;
                            }
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
        }
        ScreenAction::Pop => {
            match app.tui_mode {
                app::TuiMode::IssueBrowser => {
                    app.issue_browser_screen = None;
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
            crate::icon_mode::init_from_config(config.tui.ascii_icons);
            app.flags
                .set_enabled(crate::flags::Flag::TurboQuant, config.turboquant.enabled);
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
        ScreenAction::Quit => {
            app.running = false;
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
                    AdaptStep::Planning => {
                        if let (Some(profile), Some(report)) = (
                            screen.results.profile.clone(),
                            screen.results.report.clone(),
                        ) {
                            app.pending_commands
                                .push(app::TuiCommand::RunAdaptPlan(config, profile, report));
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
                        WorkItem::from_issue(crate::github::types::GhIssue {
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
