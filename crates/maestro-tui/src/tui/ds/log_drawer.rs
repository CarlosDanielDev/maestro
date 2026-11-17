//! Log drawer (#1263): the activity log on demand, over the bottom of the
//! main area, on screens that do not show the Activity pane. `Ctrl+L`
//! toggles it on any screen and `Esc` closes it
//! (`input_handler::handle_global_shortcuts`).

use ratatui::{Frame, layout::Rect, style::Style, widgets::Clear};

use crate::tui::activity_log::ActivityLog;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::theme::Theme;

/// The drawer box: the last `rows` rows of `area` (never more than `area`).
fn rect(area: Rect, rows: u16) -> Rect {
    let h = rows.min(area.height);
    Rect::new(area.x, area.bottom() - h, area.width, h)
}

/// Draw `log` in a `surface_overlay` drawer of `rows` rows at the bottom of
/// `area`, in local time.
pub fn draw(
    f: &mut Frame,
    area: Rect,
    rows: u16,
    log: &ActivityLog,
    theme: &Theme,
    use_nerd_font: bool,
) {
    let rect = rect(area, rows);
    if rect.height < 3 {
        return;
    }
    let block =
        panel("Activity Log", Focus::Overlay, theme).style(Style::new().bg(theme.surface_overlay));
    f.render_widget(Clear, rect);
    log.draw_block_in_zone(f, rect, block, theme, use_nerd_font, &chrono::Local);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_takes_the_bottom_rows_and_clamps() {
        let area = Rect::new(0, 2, 80, 30);
        assert_eq!(rect(area, 10), Rect::new(0, 22, 80, 10));
        assert_eq!(rect(area, 99), area);
    }
}
