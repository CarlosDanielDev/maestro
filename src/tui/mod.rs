pub mod activity_log;
pub mod app;
pub mod cost_dashboard;
pub mod dep_graph;
pub mod detail;
pub mod fullscreen;
pub mod help;
pub mod markdown;
pub mod navigation;
pub mod panels;
pub mod screens;
pub mod spinner;
pub mod theme;
pub mod ui;
pub mod widgets;

#[cfg(test)]
mod snapshot_tests;

use crate::github::client::{GhCliClient, GitHubClient};
use crate::github::types::GhIssue;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::Screen;
use app::App;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use screens::ScreenAction;
use std::io;
use std::time::Duration;

/// Run the TUI event loop.
pub async fn run(mut app: App) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Cleanup: kill any remaining sessions
    app.kill_all().await;

    // Save final state (ensures state is persisted on exit/crash)
    app.state.sessions = app.pool.all_sessions().into_iter().cloned().collect();
    app.state.update_total_cost();
    app.state.last_updated = Some(chrono::Utc::now());
    if let Err(e) = app.store.save(&app.state) {
        eprintln!("Warning: failed to save state: {}", e);
    }

    // Print session summary to stdout after TUI exits
    print_summary(&app);

    result
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    spawn_version_check(app.data_tx.clone());

    loop {
        // Draw
        terminal.draw(|f| ui::draw(f, app))?;

        // Process session events (non-blocking drain)
        while let Ok(evt) = app.event_rx.try_recv() {
            app.handle_session_event(evt);
        }

        // Check for completed sessions and promote queued ones
        app.check_completions().await?;

        // Check for keyboard/mouse input (with timeout for responsive updates)
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

                    // Help overlay intercepts all keys when visible
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

                    // CompletionSummary overlay intercepts all keys
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
                                app.prompt_input_screen = Some(screens::PromptInputScreen::new());
                                app.tui_mode = app::TuiMode::PromptInput;
                            }
                            (KeyCode::Char('l'), _) => {
                                app.tui_mode = app::TuiMode::Overview;
                            }
                            (KeyCode::Char('f'), _) => {
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
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                                app.panel_view.scroll_up();
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                                app.panel_view.scroll_down();
                            }
                            _ => {}
                        }
                        // Clear overlay when navigating away (Enter/Esc handled by transition_to_dashboard)
                        if !matches!(
                            key.code,
                            KeyCode::Up | KeyCode::Down | KeyCode::Char('k' | 'j')
                        ) {
                            app.completion_summary = None;
                        }
                        continue;
                    }

                    // ContinuousPause overlay intercepts all keys
                    if app.tui_mode == app::TuiMode::ContinuousPause {
                        match (key.code, key.modifiers) {
                            // [s] Skip — mark failed, advance to next
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
                            // [r] Retry — re-enqueue the failed issue
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
                            // [q] or Ctrl+C — stop continuous mode
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

                    // Global hotkeys that must not be swallowed by screens
                    if key.code == KeyCode::Char('?') {
                        app.show_help = true;
                        app.help_scroll = 0;
                        continue;
                    }

                    // Delegate to active screen when in screen-based modes
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
                        // Full-screen view for selected session
                        (KeyCode::Char('f'), _) => {
                            let selected = app.panel_view.selected_index();
                            app.tui_mode = app::TuiMode::Fullscreen(selected);
                        }
                        // Cost dashboard
                        (KeyCode::Char('$'), _) => {
                            app.tui_mode = app::TuiMode::CostDashboard;
                        }
                        // Tab cycles TUI modes: Overview -> DependencyGraph -> CostDashboard -> Overview
                        (KeyCode::Tab, _) => {
                            app.tui_mode = match app.tui_mode {
                                app::TuiMode::Overview => app::TuiMode::DependencyGraph,
                                app::TuiMode::DependencyGraph => app::TuiMode::CostDashboard,
                                app::TuiMode::CostDashboard => app::TuiMode::Overview,
                                app::TuiMode::Detail(_)
                                | app::TuiMode::Fullscreen(_)
                                | app::TuiMode::Dashboard
                                | app::TuiMode::IssueBrowser
                                | app::TuiMode::MilestoneView
                                | app::TuiMode::PromptInput
                                | app::TuiMode::CompletionSummary
                                | app::TuiMode::ContinuousPause => app::TuiMode::Overview,
                            };
                        }
                        // Esc returns to dashboard when no sessions are running,
                        // otherwise returns to overview
                        (KeyCode::Esc, _) => {
                            if app.home_screen.is_some() && app.pool.total_count() == 0 {
                                app.tui_mode = app::TuiMode::Dashboard;
                            } else {
                                app.tui_mode = app::TuiMode::Overview;
                            }
                        }
                        // Enter opens detail view for selected session
                        (KeyCode::Enter, _) => {
                            let selected = app.panel_view.selected_index();
                            app.tui_mode = app::TuiMode::Detail(selected);
                        }
                        // 1-9 jump to session detail by index
                        (KeyCode::Char(c), _) if c.is_ascii_digit() && c != '0' => {
                            let idx = (c as usize) - ('1' as usize);
                            if idx < app.pool.all_sessions().len() {
                                app.tui_mode = app::TuiMode::Detail(idx);
                            }
                        }
                        // Dismiss notification
                        (KeyCode::Char('d'), _) => {
                            app.notifications.dismiss_latest();
                        }
                        // Scroll activity log (Shift+arrows)
                        (KeyCode::Up, KeyModifiers::SHIFT) => {
                            app.activity_log.scroll_down();
                        }
                        (KeyCode::Down, KeyModifiers::SHIFT) => {
                            app.activity_log.scroll_up();
                        }
                        // Scroll agent panel output (plain arrows)
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

        // Drain data events from background fetches
        while let Ok(data_evt) = app.data_rx.try_recv() {
            app.handle_data_event(data_evt);
        }

        // Process pending commands
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
                    app.pending_session_launches.push(session);
                }
            }
        }

        // Launch sessions that were prepared by IssueFetched data events
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

        // In continuous mode, check if work assigner has more work after all sessions finish
        if app.all_done()
            && app.continuous_mode.is_some()
            && !matches!(
                app.tui_mode,
                app::TuiMode::ContinuousPause | app::TuiMode::CompletionSummary
            )
        {
            let all_terminal = app
                .work_assigner
                .as_ref()
                .map(|a| a.all_terminal())
                .unwrap_or(true);
            if all_terminal {
                // All milestone issues are done — end continuous mode and show summary
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
            // Otherwise, tick_work_assigner will pick the next issue on the next loop iteration
        }

        // Auto-transition when all sessions complete (fires once)
        if app.all_done()
            && app.continuous_mode.is_none()
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
            // If we have a home screen and no sessions ever launched, return to dashboard
            if app.home_screen.is_some() && app.pool.total_count() == 0 {
                app.tui_mode = app::TuiMode::Dashboard;
                continue;
            }

            // --once flag: preserve old exit behavior for CI/scripting
            if app.once_mode {
                return Ok(());
            }

            // Build summary and show overlay
            app.completion_summary = Some(app.build_completion_summary());
            app.tui_mode = app::TuiMode::CompletionSummary;
        }
    }
}

/// Dispatch an input event to the active screen, if any.
/// Returns `Some(action)` if a screen handled the event, `None` otherwise.
fn dispatch_to_active_screen(app: &mut App, event: &Event) -> Option<ScreenAction> {
    use crate::tui::navigation::InputMode;

    let screen: &mut dyn Screen = match app.tui_mode {
        app::TuiMode::Dashboard => app.home_screen.as_mut()?,
        app::TuiMode::IssueBrowser => app.issue_browser_screen.as_mut()?,
        app::TuiMode::MilestoneView => app.milestone_screen.as_mut()?,
        app::TuiMode::PromptInput => app.prompt_input_screen.as_mut()?,
        _ => return None,
    };
    let mode = screen.desired_input_mode().unwrap_or(InputMode::Normal);
    Some(screen.handle_input(event, mode))
}

/// Returns milestone issues only when navigating from `MilestoneView`.
fn milestone_issues_if_applicable(app: &App) -> Option<Vec<GhIssue>> {
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
fn handle_screen_action(app: &mut App, action: ScreenAction) {
    match action {
        ScreenAction::None => {}
        ScreenAction::Push(mode) => {
            match mode {
                app::TuiMode::IssueBrowser => {
                    if let Some(issues) = milestone_issues_if_applicable(app) {
                        app.issue_browser_screen = Some(screens::IssueBrowserScreen::new(issues));
                    } else {
                        // Fresh screen for "All Issues" — never reuse a
                        // milestone-scoped screen (fixes #117).
                        let mut screen = screens::IssueBrowserScreen::new(vec![]);
                        screen.loading = true;
                        app.issue_browser_screen = Some(screen);
                        app.pending_commands.push(app::TuiCommand::FetchIssues);
                    }
                }
                app::TuiMode::MilestoneView => {
                    if app.milestone_screen.is_none() {
                        let mut screen = screens::MilestoneScreen::new(vec![]);
                        screen.loading = true;
                        app.milestone_screen = Some(screen);
                        app.pending_commands.push(app::TuiCommand::FetchMilestones);
                    }
                }
                app::TuiMode::PromptInput => {
                    if app.prompt_input_screen.is_none() {
                        app.prompt_input_screen = Some(screens::PromptInputScreen::new());
                    }
                }
                _ => {}
            }
            app.tui_mode = mode;
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
                _ => {}
            }
            app.tui_mode = app::TuiMode::Dashboard;
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
        ScreenAction::Quit => {
            app.running = false;
        }
        ScreenAction::LaunchSession(config) => {
            app.pending_commands
                .push(app::TuiCommand::LaunchSession(config));
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchSessions(configs) => {
            app.pending_commands
                .push(app::TuiCommand::LaunchSessions(configs));
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchPromptSession(config) => {
            app.prompt_input_screen = None;
            app.pending_commands
                .push(app::TuiCommand::LaunchPromptSession(config));
            app.tui_mode = app::TuiMode::Overview;
        }
    }
}

/// Spawn a background task to fetch an issue and send the result back for session creation.
fn spawn_issue_fetch(
    tx: tokio::sync::mpsc::UnboundedSender<app::TuiDataEvent>,
    config: screens::SessionConfig,
) {
    let custom_prompt = config.custom_prompt.clone();
    match config.issue_number {
        Some(issue_number) => {
            tokio::spawn(async move {
                let client = GhCliClient::new();
                let result = client.get_issue(issue_number).await;
                let _ = tx.send(app::TuiDataEvent::Issue(result, custom_prompt));
            });
        }
        None => {
            let _ = tx.send(app::TuiDataEvent::Issue(
                Err(anyhow::anyhow!(
                    "Cannot launch session without an issue number"
                )),
                custom_prompt,
            ));
        }
    }
}

/// Spawn a non-blocking version check that sends the result via the data channel.
fn spawn_version_check(tx: tokio::sync::mpsc::UnboundedSender<app::TuiDataEvent>) {
    tokio::spawn(async move {
        use crate::updater::checker::{GitHubReleaseChecker, UpdateChecker};
        let checker = GitHubReleaseChecker::new(crate::updater::GITHUB_REPO.to_string());
        match checker.check_for_update().await {
            Ok(info) => {
                let _ = tx.send(app::TuiDataEvent::VersionCheckResult(info));
            }
            Err(e) => {
                tracing::debug!("Version check failed: {}", e);
            }
        }
    });
}

/// Spawn background binary download and installation.
fn spawn_upgrade_download(
    tx: tokio::sync::mpsc::UnboundedSender<app::TuiDataEvent>,
    info: crate::updater::ReleaseInfo,
) {
    let dest = std::env::current_exe().unwrap_or_default();
    tokio::spawn(async move {
        let installer = crate::updater::installer::Installer::new(dest);
        match installer.download_and_install(&info.download_url).await {
            Ok(backup) => {
                let _ = tx.send(app::TuiDataEvent::UpgradeResult(Ok(backup)));
            }
            Err(e) => {
                let _ = tx.send(app::TuiDataEvent::UpgradeResult(Err(e.to_string())));
            }
        }
    });
}

/// Print a summary of all sessions to stdout after the TUI exits.
fn print_summary(app: &App) {
    let summary = app.build_completion_summary();
    if summary.sessions.is_empty() {
        return;
    }

    let all_sessions = app.pool.all_sessions();

    println!();
    println!("=== Maestro Session Summary ===");
    println!();

    for (sl, session) in summary.sessions.iter().zip(all_sessions.iter()) {
        println!(
            "  {} {} {} ${:.2} {}",
            sl.status.symbol(),
            sl.label,
            sl.status.label(),
            sl.cost_usd,
            sl.elapsed,
        );

        if !session.last_message.is_empty() {
            println!("    Last: {}", session.last_message);
        }
        if !session.files_touched.is_empty() {
            println!("    Files: {}", session.files_touched.join(", "));
        }
        if sl.status == crate::session::types::SessionStatus::Errored {
            for entry in session
                .activity_log
                .iter()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                println!("    > {}", entry.message);
            }
        }
    }

    println!();
    println!("Total cost: ${:.2}", summary.total_cost_usd);
    println!();
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

    // --- Issue #86: handle_screen_action for RefreshSuggestions ---

    #[test]
    fn handle_refresh_suggestions_action_queues_fetch_suggestion_data() {
        let mut app = make_app();
        app.transition_to_dashboard();
        app.pending_commands.clear();
        // Clear loading flag so the debounce guard allows the action
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
            "loading_suggestions must be true while fetch is pending"
        );
    }

    #[test]
    fn handle_refresh_suggestions_skips_when_already_loading() {
        let mut app = make_app();
        app.transition_to_dashboard();
        // loading_suggestions is already true from transition_to_dashboard
        app.pending_commands.clear();
        handle_screen_action(&mut app, ScreenAction::RefreshSuggestions);
        assert!(
            app.pending_commands.is_empty(),
            "must not queue duplicate FetchSuggestionData while already loading"
        );
    }

    // --- Issue #117: milestone filter must not persist on All Issues view ---

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
            "navigating to All Issues must queue FetchIssues, not reuse stale screen"
        );
        let screen = app.issue_browser_screen.as_ref().unwrap();
        assert!(screen.loading, "fresh screen must be in loading state");
        assert!(
            screen.issues.is_empty(),
            "fresh screen must start with no issues"
        );
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
            "milestone push must NOT queue FetchIssues"
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
        assert_eq!(
            screen.filtered_indices.len(),
            3,
            "All Issues must show all fetched issues — no milestone filter should be active"
        );
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
        assert_eq!(
            screen.issues.len(),
            2,
            "Issue browser from milestone must only contain open issues"
        );
        assert!(
            screen.issues.iter().all(|i| i.state == "open"),
            "All issues passed to IssueBrowserScreen must have state 'open'"
        );
    }
}
