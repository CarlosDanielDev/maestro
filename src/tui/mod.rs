pub mod activity_log;
pub mod app;
pub mod dep_graph;
pub mod detail;
pub mod panels;
pub mod ui;

use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;

/// Run the TUI event loop.
pub async fn run(mut app: App) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
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

        // Check for keyboard input (with timeout for responsive updates)
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
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
                // Tab cycles TUI modes: Overview -> DependencyGraph -> Overview
                (KeyCode::Tab, _) => {
                    app.tui_mode = match app.tui_mode {
                        app::TuiMode::Overview => app::TuiMode::DependencyGraph,
                        app::TuiMode::DependencyGraph => app::TuiMode::Overview,
                        app::TuiMode::Detail(_) => app::TuiMode::Overview,
                    };
                }
                // Esc returns to overview from any mode
                (KeyCode::Esc, _) => {
                    app.tui_mode = app::TuiMode::Overview;
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

        // Auto-exit when all sessions complete
        if app.all_done() {
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
