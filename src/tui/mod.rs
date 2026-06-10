pub mod activity_log;
pub(crate) mod agent_badge;
pub(crate) mod agent_graph;
pub mod app;
mod background_tasks;
pub mod breadcrumb;
pub mod budget_banner;
pub mod budget_prespawn;
pub mod call_log;
pub mod clipboard;
pub mod clipboard_toast;
pub mod cost_dashboard;
pub mod dep_graph;
pub mod detail;
pub mod fullscreen;
pub mod help;
pub mod icons;
mod input_handler;
pub mod issue_refs;
pub mod keybinding_hints;
pub mod log_viewer;
pub mod markdown;
pub mod marquee;
pub mod navigation;
pub mod panels;
mod screen_dispatch;
pub mod screens;
pub mod session_summary;
pub mod session_switcher;
pub mod shell_launcher;
pub mod spinner;
mod summary;
mod team_runner_glue;
pub mod theme;
pub mod token_dashboard;
pub mod turboquant_dashboard;
pub mod ui;
pub mod widgets;

#[cfg(test)]
mod snapshot_tests;

use crate::config::ProviderConfig;
use crate::provider::{RepoProvider, create_provider};
use crate::tui::activity_log::LogLevel;
use app::App;
use background_tasks::{spawn_issue_fetch, spawn_version_check};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::future::Future;
use std::io;
use std::time::Duration;
use summary::print_summary;

/// Wrapper around `adapt::prompts::run_claude_print` that maps the result
/// into the `Result<String, String>` shape the wizards' background tasks
/// expect, with sensible defaults (sonnet model, current directory).
async fn run_claude_print_for_wizard(prompt: &str) -> Result<String, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    crate::adapt::prompts::run_claude_print("sonnet", prompt, &cwd)
        .await
        .map_err(|e| e.to_string())
}

fn provider_config_from_app(app: &App) -> ProviderConfig {
    app.config
        .as_ref()
        .map(|c| c.effective_provider_config())
        .unwrap_or_default()
}

async fn with_provider<T, F, Fut>(provider_config: ProviderConfig, f: F) -> anyhow::Result<T>
where
    F: FnOnce(Box<dyn RepoProvider>) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let client = create_provider(&provider_config)?;
    f(client).await
}

pub(crate) fn enter_tui_mode<W: io::Write>(out: &mut W) -> io::Result<()> {
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
}

pub(crate) fn leave_tui_mode<W: io::Write>(out: &mut W) -> io::Result<()> {
    execute!(
        out,
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )
}

/// Route a mouse wheel-up to the right scroll target (#988). On the
/// Interaction screen the wheel scrolls the chat transcript; everywhere else it
/// keeps the legacy `panel_view` behavior so no other screen regresses.
fn route_mouse_scroll_up(app: &mut App) {
    const WHEEL_LINES: usize = 3;
    if app.tui_mode == app::TuiMode::Interaction
        && let Some(screen) = app.screen_state.interaction_screen.as_mut()
    {
        screen.scroll_up(WHEEL_LINES);
    } else {
        app.panel_view.scroll_up();
    }
}

/// Wheel-down counterpart of [`route_mouse_scroll_up`] (#988).
fn route_mouse_scroll_down(app: &mut App) {
    const WHEEL_LINES: usize = 3;
    if app.tui_mode == app::TuiMode::Interaction
        && let Some(screen) = app.screen_state.interaction_screen.as_mut()
    {
        screen.scroll_down(WHEEL_LINES);
    } else {
        app.panel_view.scroll_down();
    }
}

/// Run the TUI event loop.
pub async fn run(mut app: App) -> anyhow::Result<()> {
    let no_splash = app.no_splash;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    enter_tui_mode(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // The legacy timed splash is replaced by the persistent Landing screen
    // (#290). Only intercept the Dashboard boot path — session-launching
    // subcommands (cmd_run / cmd_resume) go straight to their work view.
    if !no_splash && matches!(app.tui_mode, app::TuiMode::Dashboard) {
        if app.screen_state.landing_screen.is_none() {
            app.screen_state.landing_screen = Some(screens::LandingScreen::new());
        }
        app.tui_mode = app::TuiMode::Landing;
    }

    let result = event_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    leave_tui_mode(terminal.backend_mut())?;
    terminal.show_cursor()?;

    app.kill_all().await;

    app.state.sessions = app.pool.all_sessions().into_iter().cloned().collect();
    app.state.update_total_cost();
    app.state.last_updated = Some(chrono::Utc::now());
    let _ = app.state.compact(app.turboquant_adapter.as_deref());
    if let Err(e) = app.store.save(&app.state) {
        eprintln!("Warning: failed to save state: {}", e);
    }

    print_summary(&app);

    result
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    spawn_version_check(app.data_tx.clone());

    loop {
        // Apply session stream updates before drawing. In particular, a
        // Completed event must be visible before completion follow-up work
        // can run gates/git operations on the UI thread.
        while let Ok(evt) = app.event_rx.try_recv() {
            app.handle_session_event(evt);
        }

        // Live-tail: when the call-log pane is open with follow mode on,
        // snap the cursor to the newest entry that just landed (#886).
        if let app::TuiMode::CallLog(id) = app.tui_mode {
            let total = app
                .pool
                .get_session(id)
                .map(|s| s.call_log.len())
                .unwrap_or(0);
            app.call_log_state.reconcile_follow_tail(total);
        }

        terminal.draw(|f| ui::draw(f, app))?;

        app.check_completions().await?;

        // #741: once the Interaction terminator banner has shown for its delay,
        // auto-navigate back to the Issues list (same Pop the keymap fires).
        if app.poll_interaction_auto_nav() {
            crate::tui::screen_dispatch::handle_screen_action(
                app,
                crate::tui::screens::ScreenAction::Pop,
            );
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if let crate::updater::UpgradeState::ReadyToRestart { .. } = &app.upgrade_state
                        && key.code == KeyCode::Char('y')
                    {
                        disable_raw_mode().ok();
                        leave_tui_mode(terminal.backend_mut()).ok();
                        terminal.show_cursor().ok();
                        if let Err(e) = crate::updater::installer::restart_with_same_args() {
                            enable_raw_mode().ok();
                            enter_tui_mode(&mut io::stdout()).ok();
                            app.upgrade_state = crate::updater::UpgradeState::Failed(format!(
                                "Restart failed: {}",
                                e
                            ));
                        }
                        continue;
                    }

                    match input_handler::handle_key(app, key).await {
                        input_handler::KeyAction::Consumed => {}
                        input_handler::KeyAction::Quit => return Ok(()),
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => route_mouse_scroll_up(app),
                    MouseEventKind::ScrollDown => route_mouse_scroll_down(app),
                    _ => {}
                },
                Event::Paste(data) => {
                    app.handle_paste(&data);
                }
                _ => {}
            }
        }

        while let Ok(data_evt) = app.data_rx.try_recv() {
            app.handle_data_event(data_evt);
        }

        let commands = std::mem::take(&mut app.pending_commands);
        for cmd in commands {
            match cmd {
                app::TuiCommand::FetchIssues => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client.list_issues(&[]).await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::Issues(result));
                    });
                }
                app::TuiCommand::FetchInteractionIssue {
                    issue_number,
                    seed_prompt,
                } => {
                    background_tasks::spawn_interaction_issue_fetch(
                        app.data_tx.clone(),
                        issue_number,
                        seed_prompt,
                        provider_config_from_app(app),
                    );
                }
                app::TuiCommand::FetchSuggestionData => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            let (ready_result, failed_result, milestones_result) = tokio::join!(
                                client.list_issues(&["maestro:ready"]),
                                client.list_issues(&["maestro:failed"]),
                                client.list_milestones("open"),
                            );
                            let ready_count = ready_result.map(|v| v.len()).unwrap_or(0);
                            let failed_count = failed_result.map(|v| v.len()).unwrap_or(0);
                            let milestones_vec = milestones_result.unwrap_or_default();
                            let open_issue_count: usize =
                                milestones_vec.iter().map(|m| m.open_issues as usize).sum();
                            let closed_issue_count: usize = milestones_vec
                                .iter()
                                .map(|m| m.closed_issues as usize)
                                .sum();
                            let milestones_data: Vec<_> = milestones_vec
                                .iter()
                                .map(|m| {
                                    let total = m.open_issues + m.closed_issues;
                                    (m.title.clone(), m.closed_issues, total)
                                })
                                .collect();
                            Ok(app::SuggestionDataPayload {
                                ready_issue_count: ready_count,
                                failed_issue_count: failed_count,
                                milestones: milestones_data,
                                open_issue_count,
                                closed_issue_count,
                            })
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::SuggestionData(result));
                    });
                }
                app::TuiCommand::FetchMilestones => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            let milestones = client.list_milestones("open").await?;
                            let futures: Vec<_> = milestones
                                .iter()
                                .map(|ms| client.list_issues_by_milestone(&ms.title))
                                .collect();
                            let results = futures::future::join_all(futures).await;
                            Ok(milestones
                                .into_iter()
                                .zip(results)
                                .map(|(ms, r)| (ms, r.unwrap_or_default()))
                                .collect())
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::Milestones(result));
                    });
                }
                app::TuiCommand::LaunchSession(config) => {
                    spawn_issue_fetch(app.data_tx.clone(), config, provider_config_from_app(app));
                }
                app::TuiCommand::LaunchSessions(configs) => {
                    let provider_config = provider_config_from_app(app);
                    for config in configs {
                        spawn_issue_fetch(app.data_tx.clone(), config, provider_config.clone());
                    }
                }
                app::TuiCommand::LaunchUnifiedSession(config) => {
                    let tx = app.data_tx.clone();
                    let issue_numbers: Vec<u64> = config.issues.iter().map(|(n, _)| *n).collect();
                    let custom_prompt = config.custom_prompt.clone();
                    let agent_id = config.agent_id.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            let futures: Vec<_> = issue_numbers
                                .iter()
                                .map(|num| client.get_issue(*num))
                                .collect();
                            let results = futures::future::join_all(futures).await;
                            let mut issues = Vec::new();
                            for result in results {
                                match result {
                                    Ok(issue) => issues.push(issue),
                                    Err(e) => return Err(e),
                                }
                            }
                            Ok(issues)
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::UnifiedIssues(
                            result,
                            custom_prompt,
                            agent_id,
                        ));
                    });
                }
                app::TuiCommand::LaunchPromptSession(config) => {
                    let agent_id = config
                        .agent_id
                        .clone()
                        .unwrap_or_else(|| app.selected_agent_id());
                    let model = app
                        .config
                        .as_ref()
                        .and_then(|c| c.resolve_agent(Some(&agent_id)).ok())
                        .and_then(|resolved| resolved.config.model)
                        .unwrap_or_else(|| app.session_config.default_model.clone());
                    let mode = app.session_config.default_mode.clone();
                    let mode_config =
                        crate::modes::resolve_session_mode_config(&mode, app.config.as_ref());

                    let original_prompt = config.prompt.clone();
                    let prompt = if config.image_paths.is_empty() {
                        config.prompt
                    } else {
                        let image_refs: String = config
                            .image_paths
                            .iter()
                            .map(|p| format!("\n[Attached image: {}]", p))
                            .collect();
                        format!("{}{}", config.prompt, image_refs)
                    };

                    let session =
                        crate::session::types::Session::new(prompt, model, mode, None, None)
                            .with_mode_config(mode_config)
                            .with_agent_id(Some(agent_id));

                    // Record in prompt history
                    app.prompt_history
                        .push(crate::state::prompt_history::PromptHistoryEntry {
                            prompt: original_prompt,
                            timestamp: chrono::Utc::now(),
                            session_id: Some(session.id),
                            outcome: crate::state::prompt_history::PromptOutcome::Unknown,
                        });

                    app.pending_session_launches.push(session);
                }
                app::TuiCommand::FetchOpenPrs => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client.list_open_prs().await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::PullRequests(result));
                    });
                }
                app::TuiCommand::SubmitPrReview {
                    pr_number,
                    event,
                    body,
                } => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client.submit_pr_review(pr_number, event, &body).await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::PrReviewSubmitted(result));
                    });
                }
                app::TuiCommand::RunAdaptScan(config) => {
                    let tx = app.data_tx.clone();
                    let path = config.path.clone();
                    tokio::spawn(async move {
                        use crate::adapt::scanner::{LocalProjectScanner, ProjectScanner};
                        let scanner = LocalProjectScanner::new();
                        let result = scanner.scan(&path).await.map(Box::new);
                        let _ = tx.send(app::TuiDataEvent::AdaptScanResult(result));
                    });
                }
                app::TuiCommand::RunAdaptAnalyze(config, profile) => {
                    let tx = app.data_tx.clone();
                    let model = config.model.unwrap_or_else(|| "sonnet".to_string());
                    tokio::spawn(async move {
                        use crate::adapt::analyzer::{ClaudeAnalyzer, ProjectAnalyzer};
                        let analyzer = ClaudeAnalyzer::new(model);
                        let result = analyzer.analyze(&profile).await;
                        let _ = tx.send(app::TuiDataEvent::AdaptAnalyzeResult(result));
                    });
                }
                app::TuiCommand::RunAdaptConsolidate(config, profile, report) => {
                    let tx = app.data_tx.clone();
                    let model = config.model.unwrap_or_else(|| "sonnet".to_string());
                    tokio::spawn(async move {
                        use crate::adapt::prd::{ClaudePrdGenerator, PrdGenerator};
                        let generator = ClaudePrdGenerator::new(model);
                        let result = generator.generate(&profile, &report).await;
                        if let Ok(ref content) = result {
                            let prd_path = profile.root.join("docs/PRD.md");
                            if !prd_path.exists() {
                                if let Some(parent) = prd_path.parent()
                                    && let Err(e) = std::fs::create_dir_all(parent)
                                {
                                    tracing::warn!("Failed to create docs/: {}", e);
                                }
                                if let Err(e) = std::fs::write(&prd_path, content) {
                                    tracing::warn!("Failed to write PRD: {}", e);
                                }
                            }
                        }
                        let _ = tx.send(app::TuiDataEvent::AdaptConsolidateResult(result));
                    });
                }
                app::TuiCommand::RunAdaptPlan(config, profile, report, prd_content) => {
                    let tx = app.data_tx.clone();
                    let model = config.model.unwrap_or_else(|| "sonnet".to_string());
                    tokio::spawn(async move {
                        use crate::adapt::planner::{AdaptPlanner, ClaudePlanner};
                        let project_cfg =
                            crate::config::Config::find_and_load_in(&config.path).ok();
                        let milestone_hint = crate::adapt::detect_milestone_hint(
                            &profile.root,
                            project_cfg.as_ref(),
                        )
                        .await;
                        let planner = ClaudePlanner::new(model);
                        let result = planner
                            .plan(
                                &profile,
                                &report,
                                prd_content.as_deref(),
                                milestone_hint.as_deref(),
                            )
                            .await;
                        let _ = tx.send(app::TuiDataEvent::AdaptPlanResult(result));
                    });
                }
                app::TuiCommand::RunAdaptScaffold(config, profile, report, plan) => {
                    let tx = app.data_tx.clone();
                    let model = config.model.unwrap_or_else(|| "sonnet".to_string());
                    tokio::spawn(async move {
                        use crate::adapt::scaffolder::{ClaudeScaffolder, ProjectScaffolder};
                        let scaffolder = ClaudeScaffolder::new(model);
                        let result = scaffolder.scaffold(&profile, &report, &plan).await;
                        let _ = tx.send(app::TuiDataEvent::AdaptScaffoldResult(result));
                    });
                }
                app::TuiCommand::RunAdaptMaterialize(plan, report) => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    let provider_kind = provider_config.kind;
                    tokio::spawn(async move {
                        use crate::adapt::materializer::{PlanMaterializer, RepoMaterializer};
                        let result = with_provider(provider_config, |provider| async move {
                            let materializer =
                                RepoMaterializer::new(provider_kind, provider.as_ref());
                            materializer.materialize(&plan, &report, false).await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::AdaptMaterializeResult(result));
                    });
                }
                app::TuiCommand::CreateIssue(payload) => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        use crate::provider::CreateOutcome;
                        let body =
                            crate::tui::screens::issue_wizard::render_body_markdown(&payload);
                        let labels = crate::tui::screens::issue_wizard::render_labels(&payload);
                        let title = payload.title.clone();
                        let create_title = title.clone();
                        let milestone = payload.milestone;
                        let result = with_provider(provider_config, |client| async move {
                            client
                                .create_issue(&create_title, &body, &labels, milestone)
                                .await
                        })
                        .await;
                        let evt = match result {
                            Ok(CreateOutcome::Created(n)) => app::TuiDataEvent::IssueCreated(Ok(n)),
                            Ok(CreateOutcome::Existed { number, state }) => {
                                app::TuiDataEvent::IssueAlreadyExists {
                                    number,
                                    state,
                                    title,
                                }
                            }
                            Err(e) => app::TuiDataEvent::IssueCreated(Err(e)),
                        };
                        let _ = tx.send(evt);
                    });
                }
                app::TuiCommand::FetchWizardDependencies => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client.list_issues(&[]).await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::WizardDependencyIssues(result));
                    });
                }
                app::TuiCommand::LaunchAiReview(payload) => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let prompt =
                            crate::tui::screens::issue_wizard::build_review_prompt(&payload);
                        let res = run_claude_print_for_wizard(&prompt).await;
                        let _ = tx.send(app::TuiDataEvent::AiReviewResult(res));
                    });
                }
                app::TuiCommand::LaunchAiImprove(payload, critique) => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let prompt = crate::tui::screens::issue_wizard::build_improve_prompt(
                            &payload, &critique,
                        );
                        let res = run_claude_print_for_wizard(&prompt).await;
                        let parsed = res.and_then(|raw| {
                            crate::tui::screens::issue_wizard::parse_improve_response(
                                &payload, &raw,
                            )
                        });
                        let _ = tx.send(app::TuiDataEvent::AiImproveResult(parsed));
                    });
                }
                app::TuiCommand::CreateMilestoneWithIssues(plan) => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let res = crate::tui::screens::milestone_wizard::materialize_plan(
                            &plan,
                            provider_config,
                        )
                        .await;
                        let _ = tx.send(app::TuiDataEvent::MilestonePlanCreated(res));
                    });
                }
                app::TuiCommand::LaunchAiPlanning(payload) => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let prompt =
                            crate::tui::screens::milestone_wizard::build_planning_prompt(&payload);
                        let res = run_claude_print_for_wizard(&prompt).await;
                        let parsed = res.and_then(|raw| {
                            crate::tui::screens::milestone_wizard::parse_planning_response(&raw)
                        });
                        let _ = tx.send(app::TuiDataEvent::AiPlanningResult(parsed));
                    });
                }
                app::TuiCommand::FetchProjectStats => {
                    let tx = app.data_tx.clone();
                    let local_sessions: Vec<crate::session::types::Session> =
                        app.pool.all_sessions().into_iter().cloned().collect();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            let (
                                open_result,
                                closed_result,
                                ready_result,
                                failed_result,
                                done_result,
                                milestones_result,
                            ) = tokio::join!(
                                client.list_issues(&[]),
                                client.list_issues(&["state:closed"]),
                                client.list_issues(&["maestro:ready"]),
                                client.list_issues(&["maestro:failed"]),
                                client.list_issues(&["maestro:done"]),
                                client.list_milestones("open"),
                            );
                            Ok(crate::tui::screens::project_stats::aggregate(
                                open_result.ok().map(|v| v.len() as u32).unwrap_or(0),
                                closed_result.ok().map(|v| v.len() as u32).unwrap_or(0),
                                ready_result.ok().map(|v| v.len() as u32).unwrap_or(0),
                                failed_result.ok().map(|v| v.len() as u32).unwrap_or(0),
                                done_result.ok().map(|v| v.len() as u32).unwrap_or(0),
                                milestones_result.unwrap_or_default(),
                                &local_sessions,
                            ))
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::ProjectStats(result));
                    });
                }
                app::TuiCommand::SyncPrd => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            crate::prd::sync::GitHubPrdSyncer::new(client)
                                .fetch_current_state()
                                .await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::PrdSyncResult(result));
                    });
                }
                app::TuiCommand::SyncRoadmap => {
                    if let Some(screen) = app.screen_state.roadmap_screen.as_mut() {
                        screen.is_loading = true;
                    }
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            crate::tui::screens::roadmap::loader::load_roadmap(client.as_ref())
                                .await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::RoadmapResult(result));
                    });
                }
                app::TuiCommand::PrCreated {
                    pr_number,
                    owner,
                    repo,
                } => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let result =
                            crate::review::auto_review::run_review_cycle(pr_number, &owner, &repo)
                                .await;
                        let _ = tx.send(app::TuiDataEvent::ReviewCycleResult { pr_number, result });
                    });
                }
                app::TuiCommand::FetchMilestoneHealthIssues { milestone } => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client
                                .list_issues_by_milestone(&milestone.title)
                                .await
                                .map(|issues| (milestone, issues))
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::MilestoneHealthIssuesFetched(result));
                    });
                }
                app::TuiCommand::PatchMilestoneDescription {
                    milestone_number,
                    description,
                } => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client
                                .patch_milestone_description(milestone_number, &description)
                                .await
                        })
                        .await;
                        let _ = tx.send(app::TuiDataEvent::MilestoneHealthPatched(result));
                    });
                }
                app::TuiCommand::FetchCiErrorReview { pr_number, branch } => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    tokio::spawn(async move {
                        let result = with_provider(provider_config, |client| async move {
                            client.ci_logs_for_check(&branch).await
                        })
                        .await
                        .map_err(|e| e.to_string());
                        let _ =
                            tx.send(app::TuiDataEvent::CiErrorReviewFetched { pr_number, result });
                    });
                }
                app::TuiCommand::RunTeam {
                    scheduler,
                    app_default_agent,
                } => {
                    let tx = app.data_tx.clone();
                    let provider_config = provider_config_from_app(app);
                    let launcher: std::sync::Arc<dyn crate::session::team_runner::TeamLauncher> =
                        std::sync::Arc::new(team_runner_glue::RealTeamLauncher {
                            tx: tx.clone(),
                            provider_config,
                        });
                    let scheduler = *scheduler;
                    tokio::spawn(async move {
                        let outcome = crate::session::team_runner::run_team(
                            launcher,
                            scheduler,
                            app_default_agent,
                        )
                        .await;
                        let summary = outcome.into_apply_result();
                        let _ = tx.send(app::TuiDataEvent::TeamLaunchResult(summary));
                    });
                }
                app::TuiCommand::SendInteractionTurn {
                    issue_number,
                    prompt,
                    model,
                } => {
                    // Drive the turn on a clone of the pool's session; stream
                    // events back to the live screen, then hand the mutated
                    // clone back so the pool can persist session_id/history.
                    let Some(mut session) = app.pool.clone_active_interaction(issue_number) else {
                        tracing::warn!(
                            "SendInteractionTurn for #{issue_number} with no active session"
                        );
                        continue;
                    };
                    let tx = app.data_tx.clone();
                    // The pool's configured provider carries the transport
                    // selector (#750) and owns any parked interactive PTY
                    // children across turns (#751).
                    let provider = app.pool.provider();
                    tokio::spawn(async move {
                        use tokio_util::sync::CancellationToken;

                        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(64);
                        let forward_tx = tx.clone();
                        let forwarder = tokio::spawn(async move {
                            while let Some(event) = events_rx.recv().await {
                                let _ = forward_tx.send(app::TuiDataEvent::InteractionTurnEvent {
                                    issue_number,
                                    event,
                                });
                            }
                        });

                        let cancel = CancellationToken::new();
                        if let Err(e) = session
                            .send_turn(prompt, &model, provider, events_tx, cancel)
                            .await
                        {
                            tracing::warn!("interaction turn for #{issue_number} failed: {e}");
                        }
                        let _ = forwarder.await;
                        let _ = tx.send(app::TuiDataEvent::InteractionTurnComplete {
                            session: Box::new(session),
                        });
                    });
                }
            }
        }

        let sessions = std::mem::take(&mut app.pending_session_launches);
        for session in sessions {
            if let Err(e) = app.add_session(session).await {
                app.activity_log.push_simple(
                    "Session".into(),
                    format!("Failed to launch: {}", e),
                    LogLevel::Error,
                );
            }
        }

        if app.all_done()
            && app.continuous_mode.is_some()
            && !matches!(
                app.tui_mode,
                app::TuiMode::ContinuousPause
                    | app::TuiMode::CompletionSummary
                    | app::TuiMode::QueueExecution
            )
        {
            let all_terminal = app
                .work_assignment_service
                .as_ref()
                .map(|s| s.inner().all_terminal())
                .unwrap_or(true);
            if all_terminal {
                if let Some(ref cont) = app.continuous_mode {
                    app.activity_log.push_simple(
                        "CONTINUOUS".into(),
                        format!(
                            "Milestone complete: {} done, {} skipped, {} failed",
                            cont.completed_count,
                            cont.skipped_count,
                            cont.failures.len()
                        ),
                        LogLevel::Info,
                    );
                }
                app.continuous_mode = None;
                app.open_completion_summary();
                continue;
            }
        }

        if app.queue_executor.is_some() && app.all_done() {
            use crate::work::executor::ExecutorPhase;
            let should_advance = app
                .queue_executor
                .as_ref()
                .map(|e| matches!(e.phase(), ExecutorPhase::Running { .. }))
                .unwrap_or(false);

            if should_advance {
                let last_session_succeeded = app
                    .pool
                    .all_sessions()
                    .last()
                    .map(|s| matches!(s.status, crate::session::types::SessionStatus::Completed))
                    .unwrap_or(false);

                if last_session_succeeded {
                    if let Some(ref mut exec) = app.queue_executor {
                        exec.mark_success();
                        if exec.is_finished() {
                            app.open_completion_summary();
                        } else {
                            app.advance_queue_and_launch();
                        }
                    }
                } else if let Some(ref mut exec) = app.queue_executor {
                    exec.mark_failure();
                }
            }
        }

        // #865: clear the dismissed flag automatically once a new session
        // has entered the pool since the last dismiss. Without this, the
        // modal stays silenced forever after the first dismiss because
        // some session-arrival paths (retry, queue advance, etc.) do not
        // run through `add_session`. `completion_summary_baseline_total`
        // is captured by `dismiss_completion_summary` whenever the modal
        // is dismissed.
        if app.completion_summary_dismissed
            && app.pool.total_count() > app.completion_summary_baseline_total
        {
            app.completion_summary_dismissed = false;
        }

        let modal_on_complete = app
            .config
            .as_ref()
            .map(|c| c.tui.modal_on_complete)
            .unwrap_or(true);

        if modal_on_complete
            && app.all_done()
            && app.continuous_mode.is_none()
            && app.queue_executor.is_none()
            && app.completion_summary.is_none()
            && !app.completion_summary_dismissed
            && !matches!(
                app.tui_mode,
                app::TuiMode::Dashboard
                    | app::TuiMode::IssueBrowser
                    | app::TuiMode::PromptInput
                    | app::TuiMode::CompletionSummary
            )
        {
            if app.screen_state.home_screen.is_some() && app.pool.total_count() == 0 {
                app.tui_mode = app::TuiMode::Dashboard;
                continue;
            }

            if app.once_mode {
                return Ok(());
            }

            app.open_completion_summary();
        }
    }
}

#[cfg(test)]
pub(crate) fn make_test_app(name: &str) -> app::App {
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    let tmp = std::env::temp_dir().join(format!("{}-{}.json", name, uuid::Uuid::new_v4()));
    let store = StateStore::new(tmp);
    app::App::new(
        store,
        3,
        Box::new(MockWorktreeManager::new()),
        "bypassPermissions".into(),
        vec![],
    )
}

#[cfg(test)]
mod handle_screen_action_tests {
    use super::*;
    use crate::tui::screen_dispatch::handle_screen_action;
    use screens::ScreenAction;

    fn make_app() -> app::App {
        super::make_test_app("maestro-tui-mod-test")
    }

    #[test]
    fn open_interaction_diff_uses_merge_base_against_project_base_branch() {
        // #918: the dispatch arm computes the diff through the GitOps seam
        // with the project base branch, then opens the overlay.
        let mock = crate::git::MockGitOps {
            diff_text: "diff --git a/x.rs b/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n+hi\n".to_string(),
            ..crate::git::MockGitOps::new()
        };
        let calls = mock.diff_calls.clone();
        let mut app = make_app().with_git_ops(Box::new(mock));
        app.screen_state.interaction_screen = Some(crate::tui::screens::InteractionScreen::new());

        handle_screen_action(
            &mut app,
            ScreenAction::OpenInteractionDiff {
                worktree_path: std::path::PathBuf::from("/tmp/maestro/issue-42"),
            },
        );

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1, "exactly one diff computation");
        assert_eq!(
            recorded[0].0,
            std::path::PathBuf::from("/tmp/maestro/issue-42")
        );
        assert_eq!(recorded[0].1, "main", "base ref is the project base branch");
        drop(recorded);
        assert!(
            app.screen_state
                .interaction_screen
                .as_ref()
                .map(|s| s.diff_review_open())
                .unwrap_or(false),
            "overlay must open on success"
        );
    }

    #[test]
    fn handle_refresh_suggestions_action_queues_fetch_suggestion_data() {
        let mut app = make_app();
        app.transition_to_dashboard();
        app.pending_commands.clear();
        if let Some(ref mut screen) = app.screen_state.home_screen {
            screen.loading_suggestions = false;
        }
        handle_screen_action(&mut app, ScreenAction::RefreshSuggestions);
        assert!(
            app.pending_commands
                .iter()
                .any(|c| matches!(c, app::TuiCommand::FetchSuggestionData)),
            "RefreshSuggestions must queue FetchSuggestionData"
        );
    }

    #[test]
    fn handle_refresh_suggestions_action_sets_loading_flag_on_home_screen() {
        let mut app = make_app();
        app.transition_to_dashboard();
        if let Some(ref mut screen) = app.screen_state.home_screen {
            screen.loading_suggestions = false;
        }
        handle_screen_action(&mut app, ScreenAction::RefreshSuggestions);
        assert!(
            app.screen_state
                .home_screen
                .as_ref()
                .map(|s| s.loading_suggestions)
                .unwrap_or(false),
        );
    }

    #[test]
    fn handle_refresh_suggestions_skips_when_already_loading() {
        let mut app = make_app();
        app.transition_to_dashboard();
        app.pending_commands.clear();
        handle_screen_action(&mut app, ScreenAction::RefreshSuggestions);
        assert!(app.pending_commands.is_empty());
    }

    use crate::provider::types::Issue;
    use crate::tui::screens::milestone::MilestoneEntry;

    fn make_issue(number: u64, milestone: Option<u64>) -> Issue {
        make_issue_with_state(number, milestone, "open")
    }

    fn make_issue_with_state(number: u64, milestone: Option<u64>, state: &str) -> Issue {
        Issue {
            number,
            title: format!("Issue #{number}"),
            body: String::new(),
            labels: vec![],
            state: state.to_string(),
            html_url: String::new(),
            milestone,
            assignees: vec![],
        }
    }

    #[test]
    fn push_issue_browser_from_all_issues_resets_stale_milestone_screen() {
        let mut app = make_app();
        app.screen_state.issue_browser_screen =
            Some(screens::IssueBrowserScreen::new(vec![make_issue(
                1,
                Some(42),
            )]));
        app.screen_state.milestone_screen = None;
        app.tui_mode = app::TuiMode::Dashboard;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        assert!(
            app.pending_commands
                .iter()
                .any(|c| matches!(c, app::TuiCommand::FetchIssues)),
        );
        let screen = app.screen_state.issue_browser_screen.as_ref().unwrap();
        assert!(screen.loading);
        assert!(screen.issues.is_empty());
    }

    #[test]
    fn push_issue_browser_from_milestone_uses_milestone_issues() {
        let mut app = make_app();
        let entry = MilestoneEntry {
            number: 3,
            title: "Sprint 1".to_string(),
            description: String::new(),
            state: "open".to_string(),
            open_issues: 1,
            closed_issues: 0,
            issues: vec![make_issue(7, Some(3))],
        };
        app.screen_state.milestone_screen = Some(screens::MilestoneScreen::new(vec![entry]));
        app.tui_mode = app::TuiMode::MilestoneView;
        app.screen_state.issue_browser_screen = None;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        assert!(
            !app.pending_commands
                .iter()
                .any(|c| matches!(c, app::TuiCommand::FetchIssues)),
        );
        let screen = app.screen_state.issue_browser_screen.as_ref().unwrap();
        assert_eq!(screen.issues.len(), 1);
        assert_eq!(screen.issues[0].number, 7);
    }

    #[test]
    fn push_issue_browser_clears_milestone_filter_on_all_issues() {
        let mut app = make_app();
        let mut stale_screen = screens::IssueBrowserScreen::new(vec![]);
        stale_screen.set_milestone_filter(Some(5));
        app.screen_state.issue_browser_screen = Some(stale_screen);
        app.screen_state.milestone_screen = None;
        app.tui_mode = app::TuiMode::Dashboard;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        let fetched = vec![
            make_issue(10, None),
            make_issue(11, Some(99)),
            make_issue(12, None),
        ];
        app.handle_data_event(app::TuiDataEvent::Issues(Ok(fetched)));

        let screen = app.screen_state.issue_browser_screen.as_ref().unwrap();
        assert_eq!(screen.filtered_indices.len(), 3);
    }

    #[test]
    fn milestone_issue_browser_excludes_closed_issues() {
        let mut app = make_app();
        let entry = MilestoneEntry {
            number: 5,
            title: "Sprint 2".to_string(),
            description: String::new(),
            state: "open".to_string(),
            open_issues: 2,
            closed_issues: 1,
            issues: vec![
                make_issue_with_state(10, Some(5), "open"),
                make_issue_with_state(11, Some(5), "closed"),
                make_issue_with_state(12, Some(5), "open"),
            ],
        };
        app.screen_state.milestone_screen = Some(screens::MilestoneScreen::new(vec![entry]));
        app.tui_mode = app::TuiMode::MilestoneView;
        app.screen_state.issue_browser_screen = None;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        let screen = app.screen_state.issue_browser_screen.as_ref().unwrap();
        assert_eq!(screen.issues.len(), 2);
        assert!(screen.issues.iter().all(|i| i.state == "open"));
    }

    // --- #738: interaction launch / re-entry / send / quit ---

    fn interaction_config(issue: u64, produce_pr: bool) -> screens::SessionConfig {
        screens::SessionConfig {
            issue_number: Some(issue),
            interaction: true,
            produce_pr,
            ..Default::default()
        }
    }

    #[test]
    fn mouse_scroll_in_interaction_mode_moves_the_interaction_screen() {
        // #988: a wheel event in Interaction mode must scroll the chat
        // transcript (scroll_up takes manual control → auto_scroll off), not
        // the legacy panel_view.
        let mut app = make_app();
        app.tui_mode = app::TuiMode::Interaction;
        app.screen_state.interaction_screen = Some(screens::InteractionScreen::new());
        assert!(
            app.screen_state
                .interaction_screen
                .as_ref()
                .unwrap()
                .auto_scroll_for_test(),
            "precondition: a fresh screen tail-follows"
        );

        route_mouse_scroll_up(&mut app);

        assert!(
            !app.screen_state
                .interaction_screen
                .as_ref()
                .unwrap()
                .auto_scroll_for_test(),
            "wheel-up in Interaction mode must drive the interaction scroll"
        );
    }

    #[test]
    fn mouse_scroll_outside_interaction_mode_leaves_interaction_untouched() {
        // A wheel event in any other mode keeps the legacy panel_view routing,
        // so a present-but-inactive interaction screen must not move (no
        // regression for other screens).
        let mut app = make_app();
        app.tui_mode = app::TuiMode::Dashboard;
        app.screen_state.interaction_screen = Some(screens::InteractionScreen::new());

        route_mouse_scroll_up(&mut app);

        assert!(
            app.screen_state
                .interaction_screen
                .as_ref()
                .unwrap()
                .auto_scroll_for_test(),
            "wheel events outside Interaction mode must not touch the chat scroll"
        );
    }

    #[test]
    fn launch_interaction_creates_session_and_navigates() {
        let mut app = make_app();
        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        assert_eq!(app.pool.interaction_count(), 1);
        assert_eq!(app.tui_mode, app::TuiMode::Interaction);
        assert!(app.screen_state.interaction_screen.is_some());
        assert!(
            app.activity_log.entries().iter().any(|e| e
                .message
                .contains("#10 launched (mode: produce_pr=false, interaction=true")),
            "expected a launched activity-log line (#742 pinned format)"
        );
    }

    #[test]
    fn launch_interaction_reentry_skips_creation_and_resumes() {
        use crate::session::interaction::{TurnRecord, TurnRole};
        let mut app = make_app();
        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        // Simulate a turn landing on the pool session, then Esc nulling the screen.
        app.pool.test_push_interaction_turn(
            10,
            TurnRecord {
                role: TurnRole::User,
                content: "hi".into(),
                started_at: chrono::Utc::now(),
                finished_at: Some(chrono::Utc::now()),
            },
        );
        app.screen_state.interaction_screen = None;

        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        assert_eq!(
            app.pool.interaction_count(),
            1,
            "must not create a second session"
        );
        assert_eq!(
            app.screen_state
                .interaction_screen
                .as_ref()
                .map(|s| s.history_len()),
            Some(1),
            "re-entry must restore the existing history"
        );
        assert!(
            app.activity_log
                .entries()
                .iter()
                .any(|e| e.message.contains("#10 resumed")),
            "expected a '#10 resumed' activity-log line"
        );
    }

    #[test]
    fn launch_interaction_after_quit_creates_new_session() {
        let mut app = make_app();
        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        handle_screen_action(&mut app, ScreenAction::QuitInteraction { issue_number: 10 });
        assert_eq!(app.tui_mode, app::TuiMode::IssueBrowser);
        assert!(app.screen_state.interaction_screen.is_none());

        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        assert_eq!(
            app.pool.interaction_count(),
            2,
            "a terminated session must not be resumed"
        );
    }

    #[test]
    fn launch_interaction_with_dialog_prompt_sends_it_as_first_turn() {
        let mut app = make_app();
        // With the issue cached, the first turn is built immediately and the
        // dialog prompt rides along as a custom instruction (#946). The cache
        // *miss* path now defers + fetches (#953) — covered by
        // `screen_dispatch::interaction_launch_tests::cache_miss_defers_*`.
        app.state.issue_cache.insert(
            10,
            crate::provider::types::Issue {
                number: 10,
                title: "Cached issue".into(),
                body: "Acceptance Criteria\n- done".into(),
                labels: Vec::new(),
                state: "open".into(),
                html_url: "https://github.com/owner/repo/issues/10".into(),
                milestone: None,
                assignees: Vec::new(),
            },
        );
        let mut cfg = interaction_config(10, false);
        cfg.custom_prompt = Some("plan the work".into());
        handle_screen_action(&mut app, ScreenAction::LaunchSession(cfg));

        let screen = app.screen_state.interaction_screen.as_ref().unwrap();
        assert!(screen.is_streaming(), "dialog prompt should start a turn");
        assert_eq!(screen.history_len(), 1, "the prompt is the first User turn");
        assert!(
            app.pending_commands.iter().any(|c| matches!(
                c,
                app::TuiCommand::SendInteractionTurn { issue_number, prompt, .. }
                    if *issue_number == 10 && prompt.contains("plan the work")
            )),
            "the first turn command must carry the dialog prompt"
        );
    }

    #[test]
    fn launch_interaction_without_prompt_opens_idle_chat() {
        let mut app = make_app();
        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        let screen = app.screen_state.interaction_screen.as_ref().unwrap();
        assert!(!screen.is_streaming());
        assert_eq!(screen.history_len(), 0);
        assert!(
            !app.pending_commands
                .iter()
                .any(|c| matches!(c, app::TuiCommand::SendInteractionTurn { .. })),
            "no prompt → no auto-sent turn"
        );
    }

    #[test]
    fn resume_or_launch_issue_with_active_session_reenters_skipping_dialog() {
        let mut app = make_app();
        // Seed an active interaction session for issue 10.
        handle_screen_action(
            &mut app,
            ScreenAction::LaunchSession(interaction_config(10, false)),
        );
        app.screen_state.interaction_screen = None; // simulate having left
        app.tui_mode = app::TuiMode::IssueBrowser;

        handle_screen_action(
            &mut app,
            ScreenAction::ResumeOrLaunchIssue { issue_number: 10 },
        );
        assert_eq!(app.tui_mode, app::TuiMode::Interaction);
        assert!(app.screen_state.interaction_screen.is_some());
        assert!(
            app.activity_log
                .entries()
                .iter()
                .any(|e| e.message.contains("#10 resumed")),
            "re-entry must log #10 resumed"
        );
    }

    #[test]
    fn resume_or_launch_issue_without_session_opens_launch_dialog() {
        use crate::provider::types::Issue;
        let mut app = make_app();
        let issue = Issue {
            number: 10,
            title: "Some issue".into(),
            body: String::new(),
            labels: vec!["maestro:ready".to_string()],
            state: "open".to_string(),
            html_url: "https://example.test/10".to_string(),
            milestone: None,
            assignees: vec![],
        };
        app.screen_state.issue_browser_screen =
            Some(crate::tui::screens::IssueBrowserScreen::new(vec![issue]));
        app.tui_mode = app::TuiMode::IssueBrowser;

        handle_screen_action(
            &mut app,
            ScreenAction::ResumeOrLaunchIssue { issue_number: 10 },
        );
        assert!(
            app.screen_state
                .issue_browser_screen
                .as_ref()
                .map(|b| b.prompt_overlay.is_some())
                .unwrap_or(false),
            "no active session → the launch dialog must open"
        );
        assert_eq!(app.pool.interaction_count(), 0);
    }

    #[test]
    fn send_interaction_turn_queues_command_with_model() {
        let mut app = make_app();
        app.pending_commands.clear();
        handle_screen_action(
            &mut app,
            ScreenAction::SendInteractionTurn {
                issue_number: 10,
                prompt: "do the thing".into(),
            },
        );
        assert!(
            app.pending_commands.iter().any(|c| matches!(
                c,
                app::TuiCommand::SendInteractionTurn { issue_number, prompt, .. }
                    if *issue_number == 10 && prompt == "do the thing"
            )),
            "SendInteractionTurn action must queue the matching command"
        );
    }
}

#[cfg(test)]
mod handle_paste_tests {
    use super::*;
    use crate::tui::screen_dispatch::dispatch_paste_to_active_screen;

    fn make_app() -> app::App {
        super::make_test_app("maestro-paste-test")
    }

    #[test]
    fn dispatch_paste_routes_to_prompt_input_screen_when_active() {
        let mut app = make_app();
        app.screen_state.prompt_input_screen = Some(
            crate::tui::screens::PromptInputScreen::new().with_launch_defaults(
                app.config
                    .as_ref()
                    .map(|c| c.launch_defaults())
                    .unwrap_or((true, false)),
            ),
        );
        app.tui_mode = app::TuiMode::PromptInput;

        dispatch_paste_to_active_screen(&mut app, "hello from paste");

        let text = app
            .screen_state
            .prompt_input_screen
            .as_ref()
            .expect("prompt_input_screen must be Some")
            .editor_text();
        assert_eq!(text, "hello from paste");
    }

    #[test]
    fn dispatch_paste_preserves_embedded_newlines() {
        let mut app = make_app();
        app.screen_state.prompt_input_screen = Some(
            crate::tui::screens::PromptInputScreen::new().with_launch_defaults(
                app.config
                    .as_ref()
                    .map(|c| c.launch_defaults())
                    .unwrap_or((true, false)),
            ),
        );
        app.tui_mode = app::TuiMode::PromptInput;

        dispatch_paste_to_active_screen(&mut app, "line1\nline2\nline3");

        let text = app
            .screen_state
            .prompt_input_screen
            .as_ref()
            .unwrap()
            .editor_text();
        assert_eq!(text, "line1\nline2\nline3");
    }

    #[test]
    fn dispatch_paste_does_not_launch_session() {
        let mut app = make_app();
        app.screen_state.prompt_input_screen = Some(
            crate::tui::screens::PromptInputScreen::new().with_launch_defaults(
                app.config
                    .as_ref()
                    .map(|c| c.launch_defaults())
                    .unwrap_or((true, false)),
            ),
        );
        app.tui_mode = app::TuiMode::PromptInput;
        app.pending_commands.clear();

        dispatch_paste_to_active_screen(&mut app, "line1\nline2\n");

        assert!(
            !app.pending_commands.iter().any(|c| matches!(
                c,
                app::TuiCommand::LaunchPromptSession(_)
                    | app::TuiCommand::LaunchSession(_)
                    | app::TuiCommand::LaunchSessions(_)
                    | app::TuiCommand::LaunchUnifiedSession(_)
            )),
            "Bracketed paste must not spawn a session"
        );
    }

    #[test]
    fn app_handle_paste_with_prompt_input_active_inserts_text() {
        let mut app = make_app();
        app.screen_state.prompt_input_screen = Some(
            crate::tui::screens::PromptInputScreen::new().with_launch_defaults(
                app.config
                    .as_ref()
                    .map(|c| c.launch_defaults())
                    .unwrap_or((true, false)),
            ),
        );
        app.tui_mode = app::TuiMode::PromptInput;

        app.handle_paste("multi\nline\npaste");

        let text = app
            .screen_state
            .prompt_input_screen
            .as_ref()
            .unwrap()
            .editor_text();
        assert_eq!(text, "multi\nline\npaste");
    }

    #[test]
    fn app_handle_paste_with_dashboard_active_does_not_panic() {
        let mut app = make_app();
        app.transition_to_dashboard();

        app.handle_paste("ignored text");
    }
}
