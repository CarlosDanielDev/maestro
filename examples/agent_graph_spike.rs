//! Throwaway prototype for issue #513 / ADR 001.
//!
//! Run with:  cargo run --example agent_graph_spike --features spike
//! Quit with:  q
//!
//! This binary is gated behind --features spike and is NEVER merged to main.
//! See `docs/adr/001-agent-graph-viz.md` for cleanup instructions.

#![cfg(feature = "spike")]

use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use maestro::agent_graph_spike::{model::build_graph, render::draw_agent_graph};
use maestro::session::types::{Session, SessionStatus};

fn main() -> Result<()> {
    enable_raw_mode().context("enable_raw_mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("terminal init")?;

    let res = run(&mut terminal);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let sessions = fake_sessions();
    let (nodes, edges) = build_graph(&sessions);
    let use_braille = maestro::icon_mode::use_nerd_font();

    loop {
        terminal.draw(|f| {
            let area = f.area();
            draw_agent_graph(f, area, &nodes, &edges, use_braille);
        })?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(k) = event::read()?
            && matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            return Ok(());
        }
    }
}

fn fake_sessions() -> Vec<Session> {
    let mut s1 = Session::new(
        String::from("issue 1001"),
        String::from("sonnet"),
        String::from("orchestrator"),
        Some(1001),
    );
    s1.status = SessionStatus::Running;
    s1.files_touched = vec!["src/main.rs".into(), "src/config.rs".into()];

    let mut s2 = Session::new(
        String::from("issue 1002"),
        String::from("sonnet"),
        String::from("orchestrator"),
        Some(1002),
    );
    s2.status = SessionStatus::Running;
    s2.files_touched = vec!["src/config.rs".into(), "Cargo.toml".into()];

    let mut s3 = Session::new(
        String::from("issue 513"),
        String::from("opus"),
        String::from("orchestrator"),
        Some(513),
    );
    s3.status = SessionStatus::Completed;
    s3.files_touched = vec!["src/main.rs".into()];

    vec![s1, s2, s3]
}
