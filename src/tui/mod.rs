pub mod app;
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

    // Save final state
    if let Err(e) = app.store.save(&app.state) {
        eprintln!("Warning: failed to save state: {}", e);
    }

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

        // Check for keyboard input (with timeout for responsive updates)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
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
                    _ => {}
                }
            }
        }

        // Auto-exit when all sessions complete
        if app.all_done() {
            // Give user a moment to see the final state
            tokio::time::sleep(Duration::from_secs(2)).await;
            terminal.draw(|f| ui::draw(f, app))?;
            return Ok(());
        }
    }
}
