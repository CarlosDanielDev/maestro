use crate::mascot::animator::SystemClock;
use crate::mascot::widget::MascotWidget;
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

use crate::mascot::frames::{MASCOT_ROWS, MASCOT_WIDTH as MASCOT_WIDTH_USIZE};

const SPLASH_DURATION_MS: u64 = 1200;
const MASCOT_HEIGHT: u16 = MASCOT_ROWS as u16;
const MASCOT_WIDTH: u16 = MASCOT_WIDTH_USIZE as u16;

/// CRT green color matching the retro theme.
const SPLASH_COLOR: Color = Color::Rgb(0, 255, 65);

use crate::tui::screens::home::LOGO as SPLASH_LOGO;

/// Show a centered splash screen with mascot + MAESTRO logo.
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

            // Layout: mascot (6 rows) + gap (1) + logo (8 rows) + gap (1) + version (1) = 17
            let total_height = MASCOT_HEIGHT + 1 + 8 + 1 + 1;
            let y_start = area.y + area.height.saturating_sub(total_height) / 2;

            // Render mascot centered
            let x_mascot = area.x + area.width.saturating_sub(MASCOT_WIDTH) / 2;
            if y_start + MASCOT_HEIGHT <= area.y + area.height
                && x_mascot + MASCOT_WIDTH <= area.x + area.width
            {
                let mascot_rect = Rect::new(x_mascot, y_start, MASCOT_WIDTH, MASCOT_HEIGHT);
                let widget =
                    MascotWidget::new(MascotState::Idle, animator.frame_index(), SPLASH_COLOR);
                f.render_widget(widget, mascot_rect);
            }

            // Render MAESTRO logo below mascot
            let logo_y = y_start + MASCOT_HEIGHT + 1;
            if logo_y + 8 <= area.y + area.height {
                let logo_rect = Rect::new(area.x, logo_y, area.width, 8);
                let logo = Paragraph::new(SPLASH_LOGO)
                    .style(Style::default().fg(SPLASH_COLOR))
                    .alignment(Alignment::Center);
                f.render_widget(logo, logo_rect);
            }

            // Version text centered below logo
            let version_y = logo_y + 8;
            if version_y < area.y + area.height {
                let version_line = Line::from(vec![Span::styled(
                    format!("v{}", version),
                    Style::default().fg(Color::DarkGray),
                )]);
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
