//! Throwaway spike binary for ADR 002 (agent personalities).
//!
//! Renders two named sprites side-by-side in 80×24, demonstrating both the
//! nerd-font path and the keyword classifier from `derive_role`. Quits on `q`.
//!
//! Run:
//!     cargo run --example agent_personalities_spike --features spike
//!
//! ASCII fallback test:
//!     MAESTRO_ASCII_ICONS=1 cargo run --example agent_personalities_spike --features spike
//!
//! Removed (along with the rest of the spike module) when ADR-002's cleanup
//! commit lands. See `docs/adr/002-agent-personalities.md` § Prototype.

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};

use maestro::agent_personalities::{
    render::draw_named_sprite,
    role::{Role, derive_role},
};
use maestro::icon_mode::use_nerd_font;

fn main() -> io::Result<()> {
    // Two hand-built prompts whose keywords drive `derive_role` to two distinct
    // roles. Documented in the ADR's Prototype section so the smoke test is
    // reproducible.
    let prompt_left = "coordinate the merge of #527 and #528";
    let prompt_right = "implement #529 — loading animations";

    let role_left = derive_role(prompt_left);
    let role_right = derive_role(prompt_right);

    // Sanity gate (ADR Go-signal #5): the two prompts must classify to distinct
    // roles. If not, the prototype's own keyword corpus regressed and the
    // visual smoke test would be useless.
    assert_ne!(
        role_left, role_right,
        "spike corpus regression: {:?} == {:?}",
        role_left, role_right
    );

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let nerd_font = use_nerd_font();

    let result = run_loop(&mut terminal, role_left, role_right, nerd_font);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    role_left: Role,
    role_right: Role,
    nerd_font: bool,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(f.area());

            draw_named_sprite(f, cols[0], role_left, "agent A", nerd_font);
            draw_named_sprite(f, cols[1], role_right, "agent B", nerd_font);
        })?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            return Ok(());
        }
    }
}
