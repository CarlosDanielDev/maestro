pub mod activity_log;
pub mod app;
pub mod cost_dashboard;
pub mod dep_graph;
pub mod detail;
pub mod fullscreen;
pub mod help;
pub mod panels;
pub mod screens;
pub mod ui;

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
                    // Help overlay intercepts all keys when visible
                    if app.show_help {
                        match key.code {
                            KeyCode::Char('?') | KeyCode::Esc => app.show_help = false,
                            _ => {}
                        }
                        continue;
                    }

                    // Delegate to active screen when in screen-based modes
                    let event = Event::Key(key);
                    let screen_handled = match app.tui_mode {
                        app::TuiMode::Dashboard => {
                            if let Some(ref mut screen) = app.home_screen {
                                let action = screen.handle_input(&event);
                                handle_screen_action(app, action);
                                true
                            } else {
                                false
                            }
                        }
                        app::TuiMode::IssueBrowser => {
                            if let Some(ref mut screen) = app.issue_browser_screen {
                                let action = screen.handle_input(&event);
                                handle_screen_action(app, action);
                                true
                            } else {
                                false
                            }
                        }
                        app::TuiMode::MilestoneView => {
                            if let Some(ref mut screen) = app.milestone_screen {
                                let action = screen.handle_input(&event);
                                handle_screen_action(app, action);
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
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
                        // Help overlay
                        (KeyCode::Char('?'), _) => {
                            app.show_help = true;
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
                                | app::TuiMode::MilestoneView => app::TuiMode::Overview,
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

        // Auto-exit when all sessions complete (skip if in dashboard mode)
        if app.all_done() && !matches!(app.tui_mode, app::TuiMode::Dashboard) {
            // If we have a home screen, return to dashboard instead of exiting
            if app.home_screen.is_some() {
                app.tui_mode = app::TuiMode::Dashboard;
                continue;
            }
            // Draw final state, then wait for quit key or timeout
            terminal.draw(|f| ui::draw(f, app))?;

            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                let remaining = deadline - tokio::time::Instant::now();
                if remaining.is_zero() {
                    break;
                }
                if event::poll(remaining.min(Duration::from_millis(100)))?
                    && let Event::Key(key) = event::read()?
                {
                    match key.code {
                        // Only these keys exit
                        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => break,
                        // Arrows scroll the agent panel output
                        KeyCode::Up => app.panel_view.scroll_up(),
                        KeyCode::Down => app.panel_view.scroll_down(),
                        _ => {}
                    }
                    // Redraw after scroll
                    terminal.draw(|f| ui::draw(f, app))?;
                }
            }
            return Ok(());
        }
    }
}

/// Process a ScreenAction returned by a screen's input handler.
fn handle_screen_action(app: &mut App, action: ScreenAction) {
    match action {
        ScreenAction::None => {}
        ScreenAction::Push(mode) => {
            app.tui_mode = mode;
        }
        ScreenAction::Pop => {
            app.tui_mode = app::TuiMode::Dashboard;
        }
        ScreenAction::Quit => {
            app.running = false;
        }
        ScreenAction::LaunchSession(_config) => {
            // TODO: Wire session launch from screen config
            app.tui_mode = app::TuiMode::Overview;
        }
        ScreenAction::LaunchSessions(_configs) => {
            // TODO: Wire multi-session launch from screen configs
            app.tui_mode = app::TuiMode::Overview;
        }
    }
}

/// Print a summary of all sessions to stdout after the TUI exits.
fn print_summary(app: &App) {
    let sessions = app.pool.all_sessions();
    if sessions.is_empty() {
        return;
    }

    println!();
    println!("=== Maestro Session Summary ===");
    println!();

    for session in &sessions {
        let label = match session.issue_number {
            Some(n) => format!("#{}", n),
            None => session.id.to_string()[..8].to_string(),
        };
        println!(
            "  {} {} {} ${:.2} {}",
            session.status.symbol(),
            label,
            session.status.label(),
            session.cost_usd,
            session.elapsed_display(),
        );

        if !session.last_message.is_empty() {
            println!("    Last: {}", session.last_message);
        }
        if !session.files_touched.is_empty() {
            println!("    Files: {}", session.files_touched.join(", "));
        }
        // Show recent activity log entries for errored sessions
        if session.status == crate::session::types::SessionStatus::Errored {
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
    println!(
        "Total cost: ${:.2}",
        sessions.iter().map(|s| s.cost_usd).sum::<f64>()
    );
    println!();
}
