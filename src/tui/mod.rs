pub mod activity_log;
pub mod app;
mod background_tasks;
pub mod cost_dashboard;
pub mod dep_graph;
pub mod detail;
pub mod fullscreen;
pub mod help;
pub mod markdown;
pub mod navigation;
pub mod panels;
mod screen_dispatch;
pub mod screens;
pub mod session_switcher;
pub mod spinner;
mod summary;
pub mod theme;
pub mod token_dashboard;
pub mod ui;
pub mod widgets;

#[cfg(test)]
mod snapshot_tests;

use crate::github::client::{GhCliClient, GitHubClient};
use crate::tui::activity_log::LogLevel;
use app::App;
use background_tasks::{spawn_issue_fetch, spawn_upgrade_download, spawn_version_check};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use screen_dispatch::{dispatch_to_active_screen, handle_screen_action};
use std::io;
use std::time::Duration;
use summary::print_summary;

/// Run the TUI event loop.
pub async fn run(mut app: App) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    app.kill_all().await;

    app.state.sessions = app.pool.all_sessions().into_iter().cloned().collect();
    app.state.update_total_cost();
    app.state.last_updated = Some(chrono::Utc::now());
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
        terminal.draw(|f| ui::draw(f, app))?;

        while let Ok(evt) = app.event_rx.try_recv() {
            app.handle_session_event(evt);
        }

        app.check_completions().await?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    match &app.upgrade_state {
                        crate::updater::UpgradeState::Available(info) => {
                            if key.code == KeyCode::Char('u') {
                                let info_clone = info.clone();
                                let tx = app.data_tx.clone();
                                app.upgrade_state = crate::updater::UpgradeState::Downloading {
                                    version: info_clone.version.clone(),
                                };
                                spawn_upgrade_download(tx, info_clone);
                                continue;
                            }
                            if key.code == KeyCode::Esc {
                                app.upgrade_state = crate::updater::UpgradeState::Hidden;
                                continue;
                            }
                        }
                        crate::updater::UpgradeState::ReadyToRestart { .. } => {
                            if key.code == KeyCode::Char('y') {
                                disable_raw_mode().ok();
                                execute!(
                                    terminal.backend_mut(),
                                    LeaveAlternateScreen,
                                    DisableMouseCapture
                                )
                                .ok();
                                terminal.show_cursor().ok();
                                if let Err(e) = crate::updater::installer::restart_with_same_args()
                                {
                                    enable_raw_mode().ok();
                                    execute!(
                                        io::stdout(),
                                        EnterAlternateScreen,
                                        EnableMouseCapture
                                    )
                                    .ok();
                                    app.upgrade_state = crate::updater::UpgradeState::Failed(
                                        format!("Restart failed: {}", e),
                                    );
                                }
                                continue;
                            }
                            if key.code == KeyCode::Char('n') || key.code == KeyCode::Esc {
                                app.upgrade_state = crate::updater::UpgradeState::Hidden;
                                app.activity_log.push_simple(
                                    "UPDATE".into(),
                                    "Upgrade installed. Restart manually to use new version."
                                        .into(),
                                    crate::tui::activity_log::LogLevel::Info,
                                );
                                continue;
                            }
                        }
                        crate::updater::UpgradeState::Failed(_) => {
                            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                                app.upgrade_state = crate::updater::UpgradeState::Hidden;
                                continue;
                            }
                        }
                        _ => {}
                    }

                    if app.show_help {
                        match key.code {
                            KeyCode::Char('?') | KeyCode::Esc => {
                                app.show_help = false;
                                app.help_scroll = 0;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.help_scroll = app.help_scroll.saturating_add(1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.help_scroll = app.help_scroll.saturating_sub(1);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.tui_mode == app::TuiMode::QueueExecution {
                        use crate::work::executor::{ExecutorPhase, FailureAction};
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => {
                                app.tui_mode = app::TuiMode::Overview;
                            }
                            (KeyCode::Char('q'), _)
                            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                app.running = false;
                                return Ok(());
                            }
                            (KeyCode::Char('r'), _) => {
                                if let Some(ref mut exec) = app.queue_executor
                                    && matches!(
                                        exec.phase(),
                                        ExecutorPhase::AwaitingDecision { .. }
                                    )
                                {
                                    exec.apply_decision(FailureAction::Retry);
                                    app.advance_queue_and_launch();
                                }
                            }
                            (KeyCode::Char('s'), _) => {
                                if let Some(ref mut exec) = app.queue_executor
                                    && matches!(
                                        exec.phase(),
                                        ExecutorPhase::AwaitingDecision { .. }
                                    )
                                {
                                    exec.apply_decision(FailureAction::Skip);
                                    if exec.is_finished() {
                                        app.completion_summary =
                                            Some(app.build_completion_summary());
                                        app.tui_mode = app::TuiMode::CompletionSummary;
                                    } else {
                                        app.advance_queue_and_launch();
                                    }
                                }
                            }
                            (KeyCode::Char('a'), _) => {
                                if let Some(ref mut exec) = app.queue_executor
                                    && matches!(
                                        exec.phase(),
                                        ExecutorPhase::AwaitingDecision { .. }
                                    )
                                {
                                    exec.apply_decision(FailureAction::Abort);
                                    app.completion_summary = Some(app.build_completion_summary());
                                    app.tui_mode = app::TuiMode::CompletionSummary;
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.tui_mode == app::TuiMode::CompletionSummary {
                        match (key.code, key.modifiers) {
                            (KeyCode::Enter, _) | (KeyCode::Esc, _) => {
                                app.transition_to_dashboard();
                            }
                            (KeyCode::Char('q'), _)
                            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                app.running = false;
                                return Ok(());
                            }
                            (KeyCode::Char('i'), _) => {
                                app.completion_summary = None;
                                app.completion_summary_dismissed = true;
                                let mut screen = screens::IssueBrowserScreen::new(vec![]);
                                screen.loading = true;
                                app.issue_browser_screen = Some(screen);
                                app.pending_commands.push(app::TuiCommand::FetchIssues);
                                app.tui_mode = app::TuiMode::IssueBrowser;
                            }
                            (KeyCode::Char('r'), _) => {
                                app.prompt_input_screen = Some(
                                    app::helpers::create_prompt_input_screen(&app.prompt_history),
                                );
                                app.tui_mode = app::TuiMode::PromptInput;
                            }
                            (KeyCode::Char('l'), _) => {
                                app.tui_mode = app::TuiMode::Overview;
                            }
                            (KeyCode::Char('f'), _) => {
                                let has_suggestions = app
                                    .completion_summary
                                    .as_ref()
                                    .map(|s| s.has_conflict_suggestions())
                                    .unwrap_or(false);
                                if has_suggestions {
                                    let config = app.completion_summary.as_ref().and_then(|s| {
                                        s.suggestions.get(s.selected_suggestion).map(|sg| {
                                            screens::ConflictFixConfig {
                                                pr_number: sg.pr_number,
                                                issue_number: sg.issue_number,
                                                branch: sg.branch.clone(),
                                                conflicting_files: sg.conflicting_files.clone(),
                                            }
                                        })
                                    });
                                    if let Some(config) = config {
                                        app.spawn_conflict_fix_session(&config);
                                        app.completion_summary = None;
                                        app.tui_mode = app::TuiMode::Overview;
                                    }
                                } else {
                                    let needs_review: Vec<_> = app
                                        .completion_summary
                                        .as_ref()
                                        .into_iter()
                                        .flat_map(|s| &s.sessions)
                                        .filter(|s| {
                                            s.status
                                                == crate::session::types::SessionStatus::NeedsReview
                                        })
                                        .cloned()
                                        .collect();
                                    if !needs_review.is_empty() {
                                        for sl in &needs_review {
                                            app.spawn_gate_fix_session(sl);
                                        }
                                        app.completion_summary = None;
                                        app.tui_mode = app::TuiMode::Overview;
                                    }
                                }
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                                if let Some(ref mut summary) = app.completion_summary
                                    && !summary.suggestions.is_empty()
                                {
                                    summary.selected_suggestion =
                                        summary.selected_suggestion.saturating_sub(1);
                                }
                                app.panel_view.scroll_up();
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                                if let Some(ref mut summary) = app.completion_summary
                                    && !summary.suggestions.is_empty()
                                {
                                    let max = summary.suggestions.len().saturating_sub(1);
                                    if summary.selected_suggestion < max {
                                        summary.selected_suggestion += 1;
                                    }
                                }
                                app.panel_view.scroll_down();
                            }
                            _ => {}
                        }
                        if !matches!(
                            key.code,
                            KeyCode::Up | KeyCode::Down | KeyCode::Char('k' | 'j')
                        ) {
                            app.completion_summary = None;
                        }
                        continue;
                    }

                    if app.tui_mode == app::TuiMode::ContinuousPause {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('s'), _) => {
                                if let Some(ref mut cont) = app.continuous_mode {
                                    let skipped = cont.current_failure().map(|f| f.issue_number);
                                    cont.on_skip();
                                    if let Some(num) = skipped {
                                        app.activity_log.push_simple(
                                            "CONTINUOUS".into(),
                                            format!("Skipped #{}, advancing...", num),
                                            LogLevel::Warn,
                                        );
                                    }
                                }
                                app.tui_mode = app::TuiMode::Overview;
                            }
                            (KeyCode::Char('r'), _) => {
                                if let Some(ref mut cont) = app.continuous_mode
                                    && let Some(issue_number) = cont.on_retry()
                                {
                                    if let Some(ref mut assigner) = app.work_assigner {
                                        assigner.mark_pending_undo_cascade(issue_number);
                                    }
                                    app.activity_log.push_simple(
                                        "CONTINUOUS".into(),
                                        format!("Retrying #{}...", issue_number),
                                        LogLevel::Info,
                                    );
                                }
                                app.tui_mode = app::TuiMode::Overview;
                            }
                            (KeyCode::Char('q'), _)
                            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                app.continuous_mode = None;
                                app.running = false;
                                return Ok(());
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if key.code == KeyCode::Char('?') {
                        app.show_help = true;
                        app.help_scroll = 0;
                        continue;
                    }

                    let event = Event::Key(key);
                    let screen_handled =
                        if let Some(action) = dispatch_to_active_screen(app, &event) {
                            handle_screen_action(app, action);
                            true
                        } else {
                            false
                        };

                    if screen_handled {
                        if !app.running {
                            return Ok(());
                        }
                        continue;
                    }

                    // Handle SessionSwitcher input
                    if app.tui_mode == app::TuiMode::SessionSwitcher {
                        match key.code {
                            KeyCode::Esc => {
                                app.session_switcher = None;
                                app.tui_mode = app::TuiMode::Overview;
                            }
                            KeyCode::Up => {
                                if let Some(sw) = &mut app.session_switcher {
                                    sw.move_up();
                                }
                            }
                            KeyCode::Down => {
                                if let Some(sw) = &mut app.session_switcher {
                                    let count = {
                                        let sessions = app.pool.all_sessions();
                                        let refs: Vec<&crate::session::types::Session> = sessions;
                                        sw.filtered_sessions(&refs).len()
                                    };
                                    sw.move_down(count);
                                }
                            }
                            KeyCode::Enter => {
                                let selected_id = app.session_switcher.as_ref().and_then(|sw| {
                                    let sessions = app.pool.all_sessions();
                                    sw.selected_session(&sessions).map(|s| s.id)
                                });
                                if let Some(id) = selected_id {
                                    app.session_switcher = None;
                                    app.tui_mode = app::TuiMode::Detail(id);
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            app.running = false;
                            return Ok(());
                        }
                        #[cfg(unix)]
                        (KeyCode::Char('p'), _) => {
                            app.pause_all();
                        }
                        #[cfg(unix)]
                        (KeyCode::Char('r'), _) => {
                            app.resume_all();
                        }
                        (KeyCode::Char('k'), _) => {
                            app.kill_all().await;
                        }
                        (KeyCode::Char('f'), _) => {
                            let selected = app.panel_view.selected_index();
                            if let Some(id) = app.pool.session_id_at_index(selected) {
                                app.tui_mode = app::TuiMode::Fullscreen(id);
                            }
                        }
                        (KeyCode::Char('$'), _) => {
                            app.tui_mode = app::TuiMode::CostDashboard;
                        }
                        (KeyCode::Char('t'), _) => {
                            app.tui_mode = app::TuiMode::TokenDashboard;
                        }
                        (KeyCode::Tab, _) => {
                            app.tui_mode = match app.tui_mode {
                                app::TuiMode::Overview => app::TuiMode::DependencyGraph,
                                app::TuiMode::DependencyGraph => app::TuiMode::CostDashboard,
                                app::TuiMode::CostDashboard => app::TuiMode::TokenDashboard,
                                app::TuiMode::TokenDashboard => app::TuiMode::Overview,
                                app::TuiMode::Detail(_)
                                | app::TuiMode::Fullscreen(_)
                                | app::TuiMode::Dashboard
                                | app::TuiMode::IssueBrowser
                                | app::TuiMode::MilestoneView
                                | app::TuiMode::PromptInput
                                | app::TuiMode::CompletionSummary
                                | app::TuiMode::ContinuousPause
                                | app::TuiMode::QueueConfirmation
                                | app::TuiMode::QueueExecution
                                | app::TuiMode::HollowRetry
                                | app::TuiMode::Sanitize
                                | app::TuiMode::Settings
                                | app::TuiMode::SessionSwitcher
                                | app::TuiMode::AdaptWizard
                                | app::TuiMode::PrReview
                                | app::TuiMode::ReleaseNotes => app::TuiMode::Overview,
                            };
                        }
                        (KeyCode::Esc, _) => {
                            if app.home_screen.is_some() && app.pool.total_count() == 0 {
                                app.tui_mode = app::TuiMode::Dashboard;
                            } else {
                                app.tui_mode = app::TuiMode::Overview;
                            }
                        }
                        (KeyCode::Enter, _) => {
                            let selected = app.panel_view.selected_index();
                            if let Some(id) = app.pool.session_id_at_index(selected) {
                                app.tui_mode = app::TuiMode::Detail(id);
                            }
                        }
                        (KeyCode::Char(c), _) if c.is_ascii_digit() && c != '0' => {
                            let idx = (c as usize) - ('1' as usize);
                            if let Some(id) = app.pool.session_id_at_index(idx) {
                                app.tui_mode = app::TuiMode::Detail(id);
                            }
                        }
                        (KeyCode::Char('w'), _) => {
                            app.session_switcher =
                                Some(crate::tui::session_switcher::SessionSwitcher::default());
                            app.tui_mode = app::TuiMode::SessionSwitcher;
                        }
                        (KeyCode::Char('d'), _) => {
                            app.notifications.dismiss_latest();
                        }
                        (KeyCode::Up, KeyModifiers::SHIFT) => {
                            app.activity_log.scroll_down();
                        }
                        (KeyCode::Down, KeyModifiers::SHIFT) => {
                            app.activity_log.scroll_up();
                        }
                        (KeyCode::Up, _) => {
                            app.panel_view.scroll_up();
                        }
                        (KeyCode::Down, _) => {
                            app.panel_view.scroll_down();
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        app.panel_view.scroll_up();
                    }
                    MouseEventKind::ScrollDown => {
                        app.panel_view.scroll_down();
                    }
                    _ => {}
                },
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
                    tokio::spawn(async move {
                        let client = GhCliClient::new();
                        let result = client.list_issues(&[]).await;
                        let _ = tx.send(app::TuiDataEvent::Issues(result));
                    });
                }
                app::TuiCommand::FetchSuggestionData => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let client = GhCliClient::new();
                        let (ready_result, failed_result, milestones_result) = tokio::join!(
                            client.list_issues(&["maestro:ready"]),
                            client.list_issues(&["maestro:failed"]),
                            client.list_milestones("open"),
                        );
                        let ready_count = ready_result.map(|v| v.len()).unwrap_or(0);
                        let failed_count = failed_result.map(|v| v.len()).unwrap_or(0);
                        let milestones = milestones_result
                            .unwrap_or_default()
                            .into_iter()
                            .map(|ms| {
                                let total = ms.open_issues + ms.closed_issues;
                                (ms.title, ms.closed_issues, total)
                            })
                            .collect();
                        let _ = tx.send(app::TuiDataEvent::SuggestionData(
                            app::SuggestionDataPayload {
                                ready_issue_count: ready_count,
                                failed_issue_count: failed_count,
                                milestones,
                            },
                        ));
                    });
                }
                app::TuiCommand::FetchMilestones => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let client = GhCliClient::new();
                        match client.list_milestones("open").await {
                            Ok(milestones) => {
                                let futures: Vec<_> = milestones
                                    .iter()
                                    .map(|ms| client.list_issues_by_milestone(&ms.title))
                                    .collect();
                                let results = futures::future::join_all(futures).await;
                                let entries = milestones
                                    .into_iter()
                                    .zip(results)
                                    .map(|(ms, r)| (ms, r.unwrap_or_default()))
                                    .collect();
                                let _ = tx.send(app::TuiDataEvent::Milestones(Ok(entries)));
                            }
                            Err(e) => {
                                let _ = tx.send(app::TuiDataEvent::Milestones(Err(e)));
                            }
                        }
                    });
                }
                app::TuiCommand::LaunchSession(config) => {
                    spawn_issue_fetch(app.data_tx.clone(), config);
                }
                app::TuiCommand::LaunchSessions(configs) => {
                    for config in configs {
                        spawn_issue_fetch(app.data_tx.clone(), config);
                    }
                }
                app::TuiCommand::LaunchPromptSession(config) => {
                    let model = app
                        .config
                        .as_ref()
                        .map(|c| c.sessions.default_model.clone())
                        .unwrap_or_else(|| "opus".to_string());
                    let mode = app
                        .config
                        .as_ref()
                        .map(|c| c.sessions.default_mode.clone())
                        .unwrap_or_else(|| "orchestrator".to_string());

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

                    let session = crate::session::types::Session::new(prompt, model, mode, None);

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
                    tokio::spawn(async move {
                        let client = GhCliClient::new();
                        let result = client.list_open_prs().await;
                        let _ = tx.send(app::TuiDataEvent::PullRequests(result));
                    });
                }
                app::TuiCommand::SubmitPrReview {
                    pr_number,
                    event,
                    body,
                } => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        let client = GhCliClient::new();
                        let result = client.submit_pr_review(pr_number, event, &body).await;
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
                app::TuiCommand::RunAdaptPlan(config, profile, report) => {
                    let tx = app.data_tx.clone();
                    let model = config.model.unwrap_or_else(|| "sonnet".to_string());
                    tokio::spawn(async move {
                        use crate::adapt::planner::{AdaptPlanner, ClaudePlanner};
                        let planner = ClaudePlanner::new(model);
                        let result = planner.plan(&profile, &report).await;
                        let _ = tx.send(app::TuiDataEvent::AdaptPlanResult(result));
                    });
                }
                app::TuiCommand::RunAdaptMaterialize(plan, report) => {
                    let tx = app.data_tx.clone();
                    tokio::spawn(async move {
                        use crate::adapt::materializer::{GhMaterializer, PlanMaterializer};
                        let github = crate::github::client::GhCliClient::new();
                        let materializer = GhMaterializer::new(github);
                        let result = materializer.materialize(&plan, &report, false).await;
                        let _ = tx.send(app::TuiDataEvent::AdaptMaterializeResult(result));
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
                .work_assigner
                .as_ref()
                .map(|a| a.all_terminal())
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
                app.completion_summary = Some(app.build_completion_summary());
                app.tui_mode = app::TuiMode::CompletionSummary;
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
                            app.completion_summary = Some(app.build_completion_summary());
                            app.tui_mode = app::TuiMode::CompletionSummary;
                        } else {
                            app.advance_queue_and_launch();
                        }
                    }
                } else if let Some(ref mut exec) = app.queue_executor {
                    exec.mark_failure();
                }
            }
        }

        if app.all_done()
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
            if app.home_screen.is_some() && app.pool.total_count() == 0 {
                app.tui_mode = app::TuiMode::Dashboard;
                continue;
            }

            if app.once_mode {
                return Ok(());
            }

            app.completion_summary = Some(app.build_completion_summary());
            app.tui_mode = app::TuiMode::CompletionSummary;
        }
    }
}

#[cfg(test)]
mod handle_screen_action_tests {
    use super::*;
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;
    use screens::ScreenAction;

    fn make_app() -> app::App {
        let tmp = std::env::temp_dir().join(format!(
            "maestro-tui-mod-test-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = StateStore::new(tmp);
        app::App::new(
            store,
            3,
            Box::new(MockWorktreeManager::new()),
            "bypassPermissions".into(),
            vec![],
        )
    }

    #[test]
    fn handle_refresh_suggestions_action_queues_fetch_suggestion_data() {
        let mut app = make_app();
        app.transition_to_dashboard();
        app.pending_commands.clear();
        if let Some(ref mut screen) = app.home_screen {
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
        if let Some(ref mut screen) = app.home_screen {
            screen.loading_suggestions = false;
        }
        handle_screen_action(&mut app, ScreenAction::RefreshSuggestions);
        assert!(
            app.home_screen
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

    use crate::github::types::GhIssue;
    use crate::tui::screens::milestone::MilestoneEntry;

    fn make_issue(number: u64, milestone: Option<u64>) -> GhIssue {
        make_issue_with_state(number, milestone, "open")
    }

    fn make_issue_with_state(number: u64, milestone: Option<u64>, state: &str) -> GhIssue {
        GhIssue {
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
        app.issue_browser_screen = Some(screens::IssueBrowserScreen::new(vec![make_issue(
            1,
            Some(42),
        )]));
        app.milestone_screen = None;
        app.tui_mode = app::TuiMode::Dashboard;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        assert!(
            app.pending_commands
                .iter()
                .any(|c| matches!(c, app::TuiCommand::FetchIssues)),
        );
        let screen = app.issue_browser_screen.as_ref().unwrap();
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
        app.milestone_screen = Some(screens::MilestoneScreen::new(vec![entry]));
        app.tui_mode = app::TuiMode::MilestoneView;
        app.issue_browser_screen = None;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        assert!(
            !app.pending_commands
                .iter()
                .any(|c| matches!(c, app::TuiCommand::FetchIssues)),
        );
        let screen = app.issue_browser_screen.as_ref().unwrap();
        assert_eq!(screen.issues.len(), 1);
        assert_eq!(screen.issues[0].number, 7);
    }

    #[test]
    fn push_issue_browser_clears_milestone_filter_on_all_issues() {
        let mut app = make_app();
        let mut stale_screen = screens::IssueBrowserScreen::new(vec![]);
        stale_screen.set_milestone_filter(Some(5));
        app.issue_browser_screen = Some(stale_screen);
        app.milestone_screen = None;
        app.tui_mode = app::TuiMode::Dashboard;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        let fetched = vec![
            make_issue(10, None),
            make_issue(11, Some(99)),
            make_issue(12, None),
        ];
        app.handle_data_event(app::TuiDataEvent::Issues(Ok(fetched)));

        let screen = app.issue_browser_screen.as_ref().unwrap();
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
        app.milestone_screen = Some(screens::MilestoneScreen::new(vec![entry]));
        app.tui_mode = app::TuiMode::MilestoneView;
        app.issue_browser_screen = None;
        app.pending_commands.clear();

        handle_screen_action(&mut app, ScreenAction::Push(app::TuiMode::IssueBrowser));

        let screen = app.issue_browser_screen.as_ref().unwrap();
        assert_eq!(screen.issues.len(), 2);
        assert!(screen.issues.iter().all(|i| i.state == "open"));
    }
}
