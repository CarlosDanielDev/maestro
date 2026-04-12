use crate::mascot::animator::SystemClock;
use crate::mascot::widget::{CLAWD_ORANGE, MascotWidget};
use crate::mascot::{MascotAnimator, MascotState};
use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::io;
use std::time::{Duration, Instant};

const SPLASH_DURATION_MS: u64 = 1200;
const MASCOT_HEIGHT: u16 = 6;
const MASCOT_WIDTH: u16 = 11;

/// Show a centered splash screen with the mascot animating in idle state.
/// Dismissed on any keypress or after 1200ms.
pub fn show_splash(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    let clock = SystemClock;
    let mut animator = MascotAnimator::new(&clock);
    let start = Instant::now();

    let version = env!("CARGO_PKG_VERSION");

    loop {
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_millis(SPLASH_DURATION_MS) {
            break;
        }

        animator.tick(&clock);

        terminal.draw(|f| {
            let area = f.area();

            // Center the mascot vertically and horizontally
            let total_height = MASCOT_HEIGHT + 2; // mascot + blank + version
            let y_start = area.y + area.height.saturating_sub(total_height) / 2;
            let x_start = area.x + area.width.saturating_sub(MASCOT_WIDTH) / 2;

            // Render mascot
            if y_start + MASCOT_HEIGHT <= area.y + area.height
                && x_start + MASCOT_WIDTH <= area.x + area.width
            {
                let mascot_rect = Rect::new(x_start, y_start, MASCOT_WIDTH, MASCOT_HEIGHT);
                let widget = MascotWidget::new(MascotState::Idle, animator.frame_index());
                f.render_widget(widget, mascot_rect);
            }

            // Version text centered below mascot
            let version_y = y_start + MASCOT_HEIGHT + 1;
            if version_y < area.y + area.height {
                let version_line = Line::from(vec![
                    Span::styled("maestro", Style::default().fg(CLAWD_ORANGE)),
                    Span::styled(
                        format!("  v{}", version),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                let version_para = Paragraph::new(version_line).alignment(Alignment::Center);
                let version_rect = Rect::new(area.x, version_y, area.width, 1);
                f.render_widget(version_para, version_rect);
            }
        })?;

        // Poll for keypress — dismiss immediately on any key
        if event::poll(Duration::from_millis(50))? && matches!(event::read()?, Event::Key(_)) {
            break;
        }
    }

    Ok(())
}
