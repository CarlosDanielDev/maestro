use super::app;
use super::screens::{self, Screen, ScreenAction};
use crate::provider::types::Issue;
use crate::session::transition::TransitionReason;
use crossterm::event::Event;
use std::collections::BTreeSet;

/// Fire wizard step-entry hooks so internal transitions (Enter advances
/// inside the wizard) trigger the same fetch/launch side effects as a
/// fresh Push. Idempotent — guarded by the wizard's own `entered_*` checks.
pub(super) fn tick_wizard_step_hooks(app: &mut app::App) {
    if app.tui_mode != app::TuiMode::IssueWizard {
        return;
    }
    let (start_dep_fetch, start_review, start_improve) =
        match app.screen_state.issue_wizard_screen.as_ref() {
            Some(s) => (
                s.entered_dependencies_step(),
                s.entered_ai_review_step(),
                s.improve_requested(),
            ),
            None => (false, false, false),
        };
    if start_dep_fetch {
        if let Some(ref mut s) = app.screen_state.issue_wizard_screen {
            s.begin_dependency_fetch();
        }
        app.pending_commands
            .push(app::TuiCommand::FetchWizardDependencies);
    }
    if start_review {
        let payload = app
            .screen_state
            .issue_wizard_screen
            .as_ref()
            .map(|s| s.payload().clone());
        if let Some(payload) = payload {
            if let Some(ref mut s) = app.screen_state.issue_wizard_screen {
                s.begin_ai_review();
            }
            app.pending_commands
                .push(app::TuiCommand::LaunchAiReview(payload));
        }
    }
    if start_improve {
        let pair = app.screen_state.issue_wizard_screen.as_ref().map(|s| {
            (
                s.payload().clone(),
                s.review_text().unwrap_or("").to_string(),
            )
        });
        if let Some((payload, critique)) = pair {
            if let Some(ref mut s) = app.screen_state.issue_wizard_screen {
                s.mark_improve_enqueued();
            }
            app.pending_commands
                .push(app::TuiCommand::LaunchAiImprove(payload, critique));
        }
    }

    let needs_create = app
        .screen_state
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
            .screen_state
            .issue_wizard_screen
            .as_ref()
            .map(|s| s.payload().clone());
        if let Some(payload) = payload {
            if let Some(ref mut s) = app.screen_state.issue_wizard_screen {
                s.mark_create_enqueued();
            }
            app.pending_commands
                .push(app::TuiCommand::CreateIssue(payload));
        }
    }

    // Milestone wizard: AiStructuring auto-launch + Materializing creation.
    if app.tui_mode == app::TuiMode::MilestoneWizard {
        let (start_planning, start_creating) =
            match app.screen_state.milestone_wizard_screen.as_ref() {
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
                .screen_state
                .milestone_wizard_screen
                .as_ref()
                .map(|s| s.payload().clone());
            if let Some(payload) = payload {
                if let Some(ref mut s) = app.screen_state.milestone_wizard_screen {
                    s.start_planning();
                }
                app.pending_commands
                    .push(app::TuiCommand::LaunchAiPlanning(payload));
            }
        }
        if start_creating {
            let plan = app
                .screen_state
                .milestone_wizard_screen
                .as_ref()
                .and_then(|s| s.generated_plan().cloned());
            if let Some(plan) = plan {
                if let Some(ref mut s) = app.screen_state.milestone_wizard_screen {
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
    if matches!(app.tui_mode, app::TuiMode::Settings) {
        app.process_pending_caveman_toggle();
    }
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
        app::TuiMode::Dashboard => app.screen_state.home_screen.as_mut()?,
        app::TuiMode::Landing => app.screen_state.landing_screen.as_mut()?,
        app::TuiMode::IssueWizard => app.screen_state.issue_wizard_screen.as_mut()?,
        app::TuiMode::ProjectStats => app.screen_state.project_stats_screen.as_mut()?,
        app::TuiMode::MilestoneWizard => app.screen_state.milestone_wizard_screen.as_mut()?,
        app::TuiMode::IssueBrowser => app.screen_state.issue_browser_screen.as_mut()?,
        app::TuiMode::MilestoneView => app.screen_state.milestone_screen.as_mut()?,
        app::TuiMode::PromptInput => app.screen_state.prompt_input_screen.as_mut()?,
        app::TuiMode::QueueConfirmation => app.screen_state.queue_confirmation_screen.as_mut()?,
        app::TuiMode::HollowRetry => app.screen_state.hollow_retry_screen.as_mut()?,
        app::TuiMode::AdaptFollowUp => app.screen_state.adapt_follow_up_screen.as_mut()?,
        app::TuiMode::Sanitize => app.screen_state.sanitize_screen.as_mut()?,
        app::TuiMode::Settings => app.screen_state.settings_screen.as_mut()?,
        app::TuiMode::AdaptWizard => app.screen_state.adapt_screen.as_mut()?,
        app::TuiMode::PrReview => app.screen_state.pr_review_screen.as_mut()?,
        app::TuiMode::ReleaseNotes => app.screen_state.release_notes_screen.as_mut()?,
        app::TuiMode::MilestoneHealth => app.screen_state.milestone_health_screen.as_mut()?,
        app::TuiMode::CiErrorReview => app.screen_state.ci_error_review_screen.as_mut()?,
        app::TuiMode::TeamWizard => app.screen_state.team_wizard_screen.as_mut()?,
        _ => return None,
    };
    let mode = screen.desired_input_mode().unwrap_or(InputMode::Normal);
    let action = screen.handle_input(event, mode);
    // Drain any pending TuiCommand the milestone-health reducer enqueued.
    if matches!(app.tui_mode, app::TuiMode::MilestoneHealth)
        && let Some(s) = app.screen_state.milestone_health_screen.as_mut()
        && let Some(cmd) = s.take_pending_command()
    {
        app.pending_commands.push(cmd);
    }
    Some(action)
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
fn milestone_issues_if_applicable(app: &app::App) -> Option<Vec<Issue>> {
    if app.tui_mode != app::TuiMode::MilestoneView {
        return None;
    }
    app.screen_state.milestone_screen.as_ref().and_then(|ms| {
        ms.selected_milestone().and_then(|entry| {
            let open_issues: Vec<Issue> = entry
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

/// Re-run project-stack detection from disk, merge into the existing
/// `maestro.toml`, reload the config, and re-seed the Settings screen.
fn handle_reset_settings_from_detection(app: &mut app::App) {
    use crate::init::{FsProjectDetector, RenderOutcome, render_or_merge, walk};
    use crate::tui::activity_log::LogLevel;

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let target = match app.config_path.clone() {
        Some(p) => p,
        None => walk::find_project_root(&cwd).join("maestro.toml"),
    };

    if !target.exists() {
        app.activity_log.push_simple(
            "Settings".into(),
            format!(
                "Reset failed: no maestro.toml at {} — run `maestro init` first.",
                target.display()
            ),
            LogLevel::Warn,
        );
        return;
    }

    let project_root = target.parent().unwrap_or(&cwd).to_path_buf();
    let existing = match std::fs::read_to_string(&target) {
        Ok(s) => s,
        Err(e) => {
            app.activity_log.push_simple(
                "Settings".into(),
                format!("Reset failed reading {}: {}", target.display(), e),
                LogLevel::Error,
            );
            return;
        }
    };

    let detector = FsProjectDetector::new();
    let outcome = match render_or_merge(&detector, &project_root, Some(&existing)) {
        Ok(o) => o,
        Err(e) => {
            app.activity_log.push_simple(
                "Settings".into(),
                format!("Reset failed: {e}"),
                LogLevel::Error,
            );
            return;
        }
    };

    let RenderOutcome::Merged { stacks, report } = outcome else {
        tracing::warn!("render_or_merge returned Fresh on reset path");
        return;
    };

    if let Err(e) = std::fs::write(&target, &report.merged_toml) {
        app.activity_log.push_simple(
            "Settings".into(),
            format!("Reset failed writing {}: {}", target.display(), e),
            LogLevel::Error,
        );
        return;
    }

    let cfg = match crate::config::Config::load(&target) {
        Ok(c) => c,
        Err(e) => {
            app.activity_log.push_simple(
                "Settings".into(),
                format!("Reset wrote file but reload failed: {e}"),
                LogLevel::Error,
            );
            return;
        }
    };

    let stack_names = if stacks.is_empty() {
        "no stacks".to_string()
    } else {
        stacks.iter().map(|s| s.id()).collect::<Vec<_>>().join(", ")
    };
    let added = report.keys_added.len();
    let preserved = report.keys_preserved.len();

    if let Some(s) = app.screen_state.settings_screen.as_mut() {
        *s = crate::tui::screens::SettingsScreen::new(cfg.clone(), app.flags.clone())
            .with_config_path(target.clone());
        s.show_caveman_status(format!(
            "Detected {stack_names}; +{added} key(s), preserved {preserved} customized."
        ));
    }
    app.config = Some(cfg);
    app.activity_log.push_simple(
        "Settings".into(),
        format!(
            "Reset complete at {}: detected {stack_names}, +{added} key(s), preserved {preserved} customized.",
            target.display()
        ),
        LogLevel::Info,
    );
}

/// Normalize the `[agents]` section in `maestro.toml` with the same
/// insertion plan surfaced by `maestro doctor`.
fn handle_normalize_agent_config(app: &mut app::App) {
    use crate::init::walk;
    use crate::tui::activity_log::LogLevel;

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let target = match app.config_path.clone() {
        Some(p) => p,
        None => walk::find_project_root(&cwd).join("maestro.toml"),
    };

    if !target.exists() {
        app.activity_log.push_simple(
            "Settings".into(),
            format!(
                "Agent config normalization failed: no maestro.toml at {}.",
                target.display()
            ),
            LogLevel::Warn,
        );
        return;
    }

    let existing = match std::fs::read_to_string(&target) {
        Ok(s) => s,
        Err(e) => {
            app.activity_log.push_simple(
                "Settings".into(),
                format!(
                    "Agent config normalization failed reading {}: {}",
                    target.display(),
                    e
                ),
                LogLevel::Error,
            );
            return;
        }
    };

    let plan = match crate::config::plan_agent_config_upgrade(&existing) {
        Ok(plan) => plan,
        Err(e) => {
            app.activity_log.push_simple(
                "Settings".into(),
                format!("Agent config normalization failed: {e}"),
                LogLevel::Error,
            );
            return;
        }
    };

    if !plan.needs_update {
        if let Some(s) = app.screen_state.settings_screen.as_mut() {
            s.show_caveman_status(format!(
                "Agent config already normalized ({})",
                plan.version.label()
            ));
        }
        app.activity_log.push_simple(
            "Settings".into(),
            format!("Agent config already normalized ({})", plan.version.label()),
            LogLevel::Info,
        );
        return;
    }

    if let Err(e) = std::fs::write(&target, &plan.normalized_toml) {
        app.activity_log.push_simple(
            "Settings".into(),
            format!(
                "Agent config normalization failed writing {}: {}",
                target.display(),
                e
            ),
            LogLevel::Error,
        );
        return;
    }

    let cfg = match crate::config::Config::load(&target) {
        Ok(c) => c,
        Err(e) => {
            app.activity_log.push_simple(
                "Settings".into(),
                format!("Agent config normalization wrote file but reload failed: {e}"),
                LogLevel::Error,
            );
            return;
        }
    };

    if let Some(s) = app.screen_state.settings_screen.as_mut() {
        *s = crate::tui::screens::SettingsScreen::new(cfg.clone(), app.flags.clone())
            .with_config_path(target.clone());
        s.show_caveman_status(format!(
            "Normalized agent config from {}; +{} key(s).",
            plan.version.label(),
            plan.keys_added.len()
        ));
    }
    app.config = Some(cfg);
    app.activity_log.push_simple(
        "Settings".into(),
        format!(
            "Agent config normalized at {} from {}; +{} key(s). This uses the v0.25 [agents] schema consumed by v0.27 teams.",
            target.display(),
            plan.version.label(),
            plan.keys_added.len()
        ),
        LogLevel::Info,
    );
}

/// Process a ScreenAction returned by a screen's input handler.
pub(super) fn handle_screen_action(app: &mut app::App, action: ScreenAction) {
    match action {
        ScreenAction::None => {}
        ScreenAction::LogActivity {
            tag,
            message,
            level,
        } => {
            app.activity_log.push_simple(tag, message, level);
        }
        ScreenAction::Push(mode) => {
            match mode {
                app::TuiMode::Landing => {
                    app.screen_state
                        .landing_screen
                        .get_or_insert_with(screens::LandingScreen::new);
                }
                app::TuiMode::IssueWizard => {
                    app.screen_state
                        .issue_wizard_screen
                        .get_or_insert_with(screens::IssueWizardScreen::new);
                }
                app::TuiMode::ProjectStats => {
                    app.screen_state.project_stats_screen =
                        Some(screens::ProjectStatsScreen::new());
                    app.pending_commands
                        .push(app::TuiCommand::FetchProjectStats);
                }
                app::TuiMode::MilestoneWizard => {
                    let provider_kind = app
                        .config
                        .as_ref()
                        .map(|c| c.provider.kind)
                        .unwrap_or_default();
                    app.screen_state
                        .milestone_wizard_screen
                        .get_or_insert_with(|| {
                            screens::MilestoneWizardScreen::with_provider_kind(provider_kind)
                        });
                }
                app::TuiMode::IssueBrowser => {
                    let layout = app
                        .config
                        .as_ref()
                        .map(|c| c.tui.layout.clone())
                        .unwrap_or_default();
                    if let Some(issues) = milestone_issues_if_applicable(app) {
                        app.screen_state.issue_browser_screen =
                            Some(screens::IssueBrowserScreen::new(issues).with_layout(layout));
                    } else {
                        // Fresh screen for "All Issues" — never reuse a
                        // milestone-scoped screen (fixes #117).
                        let mut screen =
                            screens::IssueBrowserScreen::new(vec![]).with_layout(layout);
                        screen.loading = true;
                        app.screen_state.issue_browser_screen = Some(screen);
                        app.pending_commands.push(app::TuiCommand::FetchIssues);
                    }
                }
                app::TuiMode::MilestoneView if app.screen_state.milestone_screen.is_none() => {
                    let mut screen = screens::MilestoneScreen::new(vec![]);
                    screen.loading = true;
                    app.screen_state.milestone_screen = Some(screen);
                    app.pending_commands.push(app::TuiCommand::FetchMilestones);
                }
                app::TuiMode::Settings => {
                    // Re-read settings.json on entry so external edits are reflected.
                    let caveman = app.caveman_mode();
                    let config_clone = app.config.clone();
                    if let Some(config) = config_clone {
                        let mut screen = screens::SettingsScreen::new(config, app.flags.clone())
                            .with_caveman_mode(caveman);
                        if let Some(ref path) = app.config_path {
                            screen = screen.with_config_path(path.clone());
                        } else {
                            tracing::warn!(
                                "No config path resolved at boot — Settings save will surface an error"
                            );
                        }
                        app.screen_state.settings_screen = Some(screen);
                    }
                }
                app::TuiMode::AdaptWizard => {
                    let provider_kind = app
                        .config
                        .as_ref()
                        .map(|c| c.provider.kind)
                        .unwrap_or_default();
                    app.screen_state.adapt_screen = Some(
                        crate::tui::screens::adapt::AdaptScreen::with_provider_kind(provider_kind),
                    );
                }
                app::TuiMode::PrReview => {
                    app.screen_state.pr_review_screen =
                        Some(crate::tui::screens::pr_review::PrReviewScreen::new());
                    app.pending_commands.push(app::TuiCommand::FetchOpenPrs);
                }
                app::TuiMode::ReleaseNotes => {
                    app.screen_state.release_notes_screen =
                        Some(crate::tui::screens::ReleaseNotesScreen::new());
                }
                app::TuiMode::PromptInput => {
                    app.screen_state.prompt_input_screen = Some(
                        app::helpers::create_prompt_input_screen(&app.prompt_history),
                    );
                }
                app::TuiMode::MilestoneHealth => {
                    app.screen_state.milestone_health_screen =
                        Some(crate::tui::screens::milestone_health::MilestoneHealthScreen::new());
                    app.pending_commands.push(app::TuiCommand::FetchMilestones);
                }
                app::TuiMode::TeamWizard => {
                    let provider_kind = app
                        .config
                        .as_ref()
                        .map(|c| c.provider.kind)
                        .unwrap_or_default();
                    let screen = app
                        .screen_state
                        .team_wizard_screen
                        .get_or_insert_with(|| screens::TeamWizardScreen::new(provider_kind));
                    populate_team_wizard_data(screen, app.config.as_ref());
                    // Warm the issue_metas cache so the IssuePicker
                    // autocomplete (#876) renders suggestions on first
                    // entry. Previously the cache was only populated
                    // when the user visited the Issue Browser first, an
                    // undocumented prereq — surfaced during #880 manual QA.
                    if screen.issue_metas().is_empty() {
                        app.pending_commands.push(app::TuiCommand::FetchIssues);
                    }
                }
                _ => {}
            }
            app.navigate_to(mode);
            tick_wizard_step_hooks(app);
        }
        ScreenAction::Pop => {
            match app.tui_mode {
                app::TuiMode::IssueBrowser => {
                    app.screen_state.issue_browser_screen = None;
                }
                app::TuiMode::IssueWizard => {
                    app.screen_state.issue_wizard_screen = None;
                }
                app::TuiMode::ProjectStats => {
                    app.screen_state.project_stats_screen = None;
                }
                app::TuiMode::MilestoneWizard => {
                    app.screen_state.milestone_wizard_screen = None;
                }
                app::TuiMode::TeamWizard => {
                    app.screen_state.team_wizard_screen = None;
                }
                app::TuiMode::MilestoneView => {
                    app.screen_state.milestone_screen = None;
                }
                app::TuiMode::PromptInput => {
                    app.screen_state.prompt_input_screen = None;
                }
                app::TuiMode::QueueConfirmation => {
                    app.screen_state.queue_confirmation_screen = None;
                }
                app::TuiMode::HollowRetry => {
                    app.screen_state.hollow_retry_screen = None;
                }
                app::TuiMode::AdaptFollowUp => {
                    app.screen_state.adapt_follow_up_screen = None;
                }
                app::TuiMode::CiErrorReview => {
                    app.screen_state.ci_error_review_screen = None;
                }
                app::TuiMode::Sanitize => {
                    app.screen_state.sanitize_screen = None;
                }
                app::TuiMode::Settings => {
                    app.preview_theme = None;
                    app.screen_state.settings_screen = None;
                }
                app::TuiMode::AdaptWizard => {
                    app.screen_state.adapt_screen = None;
                }
                app::TuiMode::PrReview => {
                    app.screen_state.pr_review_screen = None;
                }
                app::TuiMode::ReleaseNotes => {
                    app.screen_state.release_notes_screen = None;
                }
                app::TuiMode::MilestoneHealth => {
                    app.screen_state.milestone_health_screen = None;
                }
                _ => {}
            }
            app.navigate_back_or_dashboard();
        }
        ScreenAction::RefreshSuggestions => {
            let already_loading = app
                .screen_state
                .home_screen
                .as_ref()
                .is_some_and(|s| s.loading_suggestions);
            if !already_loading {
                if let Some(ref mut screen) = app.screen_state.home_screen {
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
            // Default-provider change is live for new sessions but old
            // sessions keep their spawn-time agent. Surface both facts
            // in the activity log so the user knows they don't need to
            // restart maestro.
            let default_provider_changed = app
                .config
                .as_ref()
                .map(|c| c.agents.default != config.agents.default)
                .unwrap_or(false);
            let new_default_provider = config.agents.default.clone();

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
            let new_permission_mode = config.effective_default_permission_mode();
            app.pool.set_permission_mode(new_permission_mode.clone());
            app.pool
                .set_allowed_tools(config.sessions.allowed_tools.clone());
            app.session_config.apply_config(&config.sessions);
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
                config.effective_default_model(),
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
            if default_provider_changed {
                // Honor the "Live for new sessions" promise: refresh
                // `App.selected_agent_id` and the pool's per-agent
                // provider map so the very next spawn picks the new
                // default. Previously this only ran at startup via
                // `App::configure`, which forced a restart to take
                // effect.
                app.apply_agents_config(&config);
                app.activity_log.push_simple(
                    "SETTINGS".into(),
                    format!(
                        "Default provider → `{new_default_provider}`. Live for new sessions; running sessions keep their original provider until they finish."
                    ),
                    crate::tui::activity_log::LogLevel::Info,
                );
            }

            app.config = Some(*config);
        }
        ScreenAction::ResetSettingsFromDetection => {
            handle_reset_settings_from_detection(app);
        }
        ScreenAction::NormalizeAgentConfig => {
            handle_normalize_agent_config(app);
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
            let config = config.with_agent_id(app.selected_agent_id());
            app.pending_commands
                .push(app::TuiCommand::LaunchUnifiedSession(config));
            // Replace the current wizard (PromptInput/IssueBrowser/etc.) with
            // Overview WITHOUT clearing the nav stack. The wizard is current,
            // not on the stack, so the stable anchors below it (Landing,
            // Dashboard) survive — Esc from the post-session Overview pops
            // back through them as the user expects. Wiping the stack
            // (previous behavior) lost the Welcome breadcrumb at every
            // session start. Reported 2026-05-23.
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchSession(config) => {
            let config = config.with_agent_id(app.selected_agent_id());
            app.pending_commands
                .push(app::TuiCommand::LaunchSession(config));
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchTeam {
            team_name,
            input,
            max_parallel,
        } => {
            // #877 — wire `launch_dispatch` to a real fan-out. v1: re-resolve
            // the team from the wizard's cache, build the Scheduler to get
            // the level DAG, then fan out one `LaunchSession` per planned
            // issue. The team-runner consolidation (proper `SessionManager::
            // run_team` with L2 routing) lands in a follow-up.
            let agent_id = app.selected_agent_id();
            let scheduler_outcome = {
                let Some(screen) = app.screen_state.team_wizard_screen.as_mut() else {
                    tracing::warn!("LaunchTeam dispatched without an active team wizard");
                    return;
                };
                let Some(team) = screen.resolved_teams().get(&team_name).cloned() else {
                    screen.apply_launch_result(Err(format!(
                        "Team `{}` no longer in wizard cache",
                        screens::sanitize_for_terminal(&team_name)
                    )));
                    return;
                };
                let issue_metas = screen.issue_metas().clone();
                let result = crate::orchestration::scheduler::Scheduler::from_input(
                    team.clone(),
                    input,
                    issue_metas,
                    max_parallel.max(1),
                );
                match result {
                    Ok(scheduler) => {
                        let configs: Vec<screens::SessionConfig> = scheduler
                            .run
                            .plan
                            .iter()
                            .flat_map(|level| level.iter())
                            .map(|n| screens::SessionConfig {
                                issue_number: Some(*n),
                                title: format!("#{n}"),
                                custom_prompt: None,
                                agent_id: Some(agent_id.clone()),
                            })
                            .collect();
                        if configs.is_empty() {
                            screen.apply_launch_result(Err(
                                "Scheduler produced an empty plan".to_string()
                            ));
                            None
                        } else {
                            screen.apply_launch_result(Ok(()));
                            tracing::warn!(
                                target: "team_wizard.launch",
                                team = ?team.name,
                                "LaunchTeam fanned out via LaunchSession path \
                                 (real SessionManager::run_team pending follow-up)"
                            );
                            Some(configs)
                        }
                    }
                    Err(e) => {
                        screen.apply_launch_result(Err(format!("Scheduler error: {e}")));
                        None
                    }
                }
            };
            if let Some(configs) = scheduler_outcome {
                app.pending_commands
                    .push(app::TuiCommand::LaunchSessions(configs));
            }
        }
        ScreenAction::LaunchSessions(configs) => {
            let agent_id = app.selected_agent_id();
            let configs = configs
                .into_iter()
                .map(|config| config.with_agent_id(agent_id.clone()))
                .collect();
            app.pending_commands
                .push(app::TuiCommand::LaunchSessions(configs));
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchPromptSession(config) => {
            let config = config.with_agent_id(app.selected_agent_id());
            app.screen_state.prompt_input_screen = None;
            app.screen_state.adapt_follow_up_screen = None;
            app.pending_commands
                .push(app::TuiCommand::LaunchPromptSession(config));
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchConflictFix(config) => {
            app.spawn_conflict_fix_session(&config);
            app.completion_summary = None;
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchCiFix(config) => {
            app.launch_ci_fix_from_review(&config);
            app.screen_state.ci_error_review_screen = None;
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::RetryHollow(session_id) => {
            // Queue a retry for the hollow session. By the time the user
            // presses [r] the hollow session has already been moved to
            // `finished`, so `pool.get_active_mut` returns None (#869).
            // Look it up across every bucket via `get_session_mut`.
            let policy = app
                .config
                .as_ref()
                .map(|c| crate::session::retry::RetryPolicy::from_config(&c.sessions));
            let progress = app.progress_tracker.get(&session_id).cloned();
            let retry_payload = policy.and_then(|policy| {
                app.pool.get_session_mut(session_id).map(|session| {
                    let label = crate::tui::app::helpers::session_label(session);
                    let retry = policy.prepare_retry(session, progress.as_ref(), None);
                    let _ = session.transition_to(
                        crate::session::types::SessionStatus::Retrying,
                        TransitionReason::RetryTriggered,
                    );
                    (retry, label)
                })
            });
            if let Some((retry, label)) = retry_payload {
                app.activity_log.push_simple(
                    label,
                    "Manual retry (hollow completion)".into(),
                    crate::tui::activity_log::LogLevel::Warn,
                );
                app.pending_session_launches.push(retry);
            }
            app.screen_state.hollow_retry_screen = None;
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::FetchPrDetail(pr_number) => {
            let pr = app
                .screen_state
                .pr_review_screen
                .as_ref()
                .and_then(|s| s.find_pr(pr_number));
            if let (Some(pr), Some(ref mut screen)) =
                (pr, app.screen_state.pr_review_screen.as_mut())
            {
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
            let mut wizard = app
                .screen_state
                .issue_wizard_screen
                .take()
                .unwrap_or_default();
            {
                let payload = wizard.payload_mut();
                payload.milestone = Some(milestone);
                payload.blocked_by = suggested_blocked_by;
            }
            app.screen_state.issue_wizard_screen = Some(wizard);
            app.navigate_to(app::TuiMode::IssueWizard);
        }
        ScreenAction::PushTeamWizard { mode, preselect } => {
            let provider_kind = app
                .config
                .as_ref()
                .map(|c| c.provider.kind)
                .unwrap_or_default();
            let mut screen = screens::TeamWizardScreen::with_entry(provider_kind, mode, preselect);
            populate_team_wizard_data(&mut screen, app.config.as_ref());
            let need_metas_fetch = screen.issue_metas().is_empty();
            app.screen_state.team_wizard_screen = Some(screen);
            app.navigate_to(app::TuiMode::TeamWizard);
            if need_metas_fetch {
                app.pending_commands.push(app::TuiCommand::FetchIssues);
            }
        }
        ScreenAction::StartAdaptPipeline(config) => {
            if let Some(ref mut screen) = app.screen_state.adapt_screen {
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

            let agent_id = app.selected_agent_id();
            let configs: Vec<_> = configs
                .into_iter()
                .map(|config| config.with_agent_id(agent_id.clone()))
                .collect();

            // Build a WorkQueue from the session configs for the executor
            let issue_numbers: Vec<u64> = configs.iter().filter_map(|c| c.issue_number).collect();

            // Build a minimal dependency graph (items are already validated by QueueConfirmation)
            let items: Vec<WorkItem> = configs
                .iter()
                .filter_map(|c| {
                    c.issue_number.map(|n| {
                        WorkItem::from_issue(crate::provider::types::Issue {
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

/// Build the known-agent list visible in the Team Wizard's Compose picker.
/// Unions the team-side agent ids (already extracted into `team_agents`)
/// with the user's `[agents.*]` config — but **excludes** entries flagged
/// `enabled = false` (#806). Pure function; test seam for the disabled-agent
/// filter contract.
pub(crate) fn build_known_agents_from_config(
    mut team_agents: BTreeSet<String>,
    agents: Option<&crate::config::AgentsConfig>,
) -> Vec<String> {
    if let Some(cfg) = agents {
        for (id, entry) in &cfg.entries {
            if entry.enabled {
                team_agents.insert(id.clone());
            }
        }
    }
    team_agents.into_iter().collect()
}

/// Synchronously populate the team-wizard screen with resolved teams, the
/// known-agent set, and a best-effort health-check cache so the Roles step
/// has agents to bind. Real `doctor::run_health_check` async wiring is a
/// follow-up; this seed unblocks the Compose flow today by trusting the
/// user's `[agents.*]` config + the agent IDs referenced in resolved teams.
fn populate_team_wizard_data(
    screen: &mut screens::TeamWizardScreen,
    config: Option<&crate::config::Config>,
) {
    use crate::agent_provider::types::{AgentHealthCheck, AgentProviderId};
    use crate::orchestration::loader::Loader;
    use std::collections::BTreeSet;

    let loader = Loader::default_for_cwd();

    let resolved = match loader.resolve() {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(error = %e, "team_wizard: Loader::resolve failed");
            std::collections::HashMap::new()
        }
    };

    let mut team_agents: BTreeSet<String> = BTreeSet::new();
    for team in resolved.values() {
        for agent in &team.min_agents {
            team_agents.insert(agent.clone());
        }
        for binding in team.bindings.values() {
            if !binding.agent.is_empty() {
                team_agents.insert(binding.agent.clone());
            }
        }
    }
    let known_agents = build_known_agents_from_config(team_agents, config.map(|c| &c.agents));

    let teams: Vec<_> = resolved.into_values().collect();
    screen.apply_resolved_teams(teams);
    let health: Vec<AgentHealthCheck> = known_agents
        .iter()
        .map(|id| AgentHealthCheck {
            provider_id: AgentProviderId::new(id),
            available: true,
            version: None,
            message: "seeded (real health check pending)".into(),
        })
        .collect();
    screen.set_known_agents(known_agents);
    screen.apply_health_check(health);
}

#[cfg(test)]
mod build_known_agents_tests {
    use super::build_known_agents_from_config;
    use crate::config::{AgentConfig, AgentKind, AgentsConfig};
    use std::collections::{BTreeMap, BTreeSet};

    fn mk_agent(kind: AgentKind, enabled: bool) -> AgentConfig {
        AgentConfig {
            kind,
            enabled,
            command: Some("placeholder".into()),
            base_url: None,
            model: None,
            env: BTreeMap::new(),
            extra_args: Vec::new(),
            permission_mode: None,
            allowed_tools: Vec::new(),
            sandbox: None,
            json: None,
            ephemeral: None,
            profile: None,
            config_overrides: BTreeMap::new(),
            cli_flags: BTreeMap::new(),
            request_timeout_secs: None,
            api_key_env: None,
            num_ctx: None,
        }
    }

    fn agents_with(entries: Vec<(&str, AgentKind, bool)>) -> AgentsConfig {
        let mut map = BTreeMap::new();
        for (id, kind, enabled) in entries {
            map.insert(id.to_string(), mk_agent(kind, enabled));
        }
        AgentsConfig {
            default: "claude".to_string(),
            entries: map,
        }
    }

    #[test]
    fn excludes_disabled_agents() {
        let cfg = agents_with(vec![
            ("qwen-enabled", AgentKind::Qwen, true),
            ("qwen-disabled", AgentKind::Qwen, false),
        ]);
        let result = build_known_agents_from_config(BTreeSet::new(), Some(&cfg));
        assert!(result.contains(&"qwen-enabled".to_string()));
        assert!(!result.contains(&"qwen-disabled".to_string()));
    }

    #[test]
    fn team_side_agents_pass_through_even_with_disabled_config() {
        let cfg = agents_with(vec![("qwen-disabled", AgentKind::Qwen, false)]);
        let team_agents = BTreeSet::from(["claude".to_string()]);
        let result = build_known_agents_from_config(team_agents, Some(&cfg));
        assert!(result.contains(&"claude".to_string()));
        assert!(!result.contains(&"qwen-disabled".to_string()));
    }

    #[test]
    fn enabled_config_agents_union_with_team_agents() {
        let cfg = agents_with(vec![("qwen-fast", AgentKind::Qwen, true)]);
        let team_agents = BTreeSet::from(["claude".to_string()]);
        let result = build_known_agents_from_config(team_agents, Some(&cfg));
        assert!(result.contains(&"claude".to_string()));
        assert!(result.contains(&"qwen-fast".to_string()));
    }

    #[test]
    fn no_config_returns_team_agents_only() {
        let team_agents = BTreeSet::from(["claude".to_string()]);
        let result = build_known_agents_from_config(team_agents, None);
        assert_eq!(result, vec!["claude".to_string()]);
    }
}
