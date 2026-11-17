//! One-row bar with a left part and a right-aligned part. Shared by the
//! TabBar, CrumbBar and footer (`ds::tab_bar`, `ds::crumb_bar`,
//! `ds::key_hints`).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Paragraph,
};

use crate::tui::theme::Theme;

/// The key part of a hint (`Esc`, `q`, a tab digit): bold `keybind_key`.
pub fn key_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.keybind_key)
        .add_modifier(Modifier::BOLD)
}

/// Draw `left` from the left edge and `right` flush right. The right part is
/// dropped when both do not fit with one column between them; the left part
/// is cut at the right part's edge.
pub fn draw_left_right(f: &mut Frame, area: Rect, left: Line<'_>, right: Line<'_>) {
    let right_w = right.width() as u16;
    let fits = left.width() as u16 + right_w < area.width;
    let left_w = if fits {
        area.width - right_w
    } else {
        area.width
    };
    f.render_widget(
        Paragraph::new(left),
        Rect {
            width: left_w,
            ..area
        },
    );
    if fits {
        let right_area = Rect {
            x: area.x + left_w,
            width: right_w,
            ..area
        };
        f.render_widget(Paragraph::new(right), right_area);
    }
}
