//! Compact landing header (#1226): one brand line instead of the 6-row logo
//! when the area is wide enough for the Release Console but too short for
//! the big logo and the Small console together.

use super::RenderMode;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Narrowest area that shows the Release Console.
pub(super) const CONSOLE_MIN_WIDTH: u16 = 80;
/// Shortest area for the compact header + Small console: 2 + 6 + 1 + 9 + 2.
pub(super) const COMPACT_CONSOLE_MIN_HEIGHT: u16 = 20;
/// From this height the big logo and the Small console fit together (#1224).
const BIG_HEADER_CONSOLE_MIN_HEIGHT: u16 = 26;
/// Brand line + one blank row.
pub(super) const COMPACT_HEADER_HEIGHT: u16 = 2;

pub(crate) fn uses_compact_header(area: Rect) -> bool {
    area.width >= CONSOLE_MIN_WIDTH
        && (COMPACT_CONSOLE_MIN_HEIGHT..BIG_HEADER_CONSOLE_MIN_HEIGHT).contains(&area.height)
}

/// Draws ` MAESTRO  vX.Y.Z` centred on the row `COMPACT_HEADER_HEIGHT` above
/// the menu. Brand tokens only, like the status bar badge; plain ASCII.
pub(super) fn draw_compact_header(
    f: &mut Frame,
    area: Rect,
    menu_y: u16,
    theme: &Theme,
    mode: RenderMode,
) {
    let y = menu_y.saturating_sub(COMPACT_HEADER_HEIGHT);
    if y >= area.y + area.height {
        return;
    }
    let (brand, version) = if mode.styled {
        (
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.branding_bg)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(theme.text_secondary),
        )
    } else {
        (Style::default(), Style::default())
    };
    let line = Line::from(vec![
        Span::styled(" MAESTRO ", brand),
        Span::raw(" "),
        Span::styled(concat!("v", env!("CARGO_PKG_VERSION")), version),
    ])
    .centered();
    f.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
