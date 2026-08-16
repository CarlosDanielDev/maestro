//! Render path for the canonical `Input` (#963).
//!
//! Reuses `screens::wrap` (`wrap_lines` + `scroll_offset_for_cursor`) so the
//! widget wraps and scrolls identically to the prompt editor, then places the
//! terminal cursor manually. The wrap/scroll math is factored into `visible`
//! so it stays unit-testable without a `Frame`.

use super::core::Input;
use crate::tui::screens::wrap::{scroll_offset_for_cursor, wrap_lines};
use crate::tui::theme::Theme;
use ratatui::{Frame, layout::Rect, style::Style, text::Line, widgets::Paragraph};

/// Wrap the input to `viewport_width`, scroll so the cursor row is visible,
/// and return `(visible visual lines, cursor position relative to the
/// viewport top-left)`. Pure — no terminal needed.
pub(crate) fn visible(
    input: &Input,
    viewport_width: u16,
    visible_height: u16,
) -> (Vec<String>, (u16, u16)) {
    let (row, col) = input.cursor();
    let wrapped = wrap_lines(input.lines(), (row, col), viewport_width);
    let height = (visible_height as usize).max(1);
    let offset = scroll_offset_for_cursor(wrapped.cursor.0 as usize, height);
    let vis: Vec<String> = wrapped
        .lines
        .into_iter()
        .skip(offset)
        .take(height)
        .collect();
    let cur_row = (wrapped.cursor.0 as usize).saturating_sub(offset) as u16;
    (vis, (cur_row, wrapped.cursor.1))
}

/// Draw the input into `area`. Renders placeholder text when empty; when
/// `focused`, positions the terminal cursor.
pub fn render(input: &Input, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if input.is_empty() && !input.placeholder().is_empty() {
        let ph = Paragraph::new(Line::from(input.placeholder().to_string()))
            .style(Style::default().fg(theme.text_muted));
        f.render_widget(ph, area);
        if focused {
            f.set_cursor_position((area.x, area.y));
        }
        return;
    }

    let (lines, (cur_row, cur_col)) = visible(input, area.width, area.height);
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(theme.text_primary)),
        area,
    );

    if focused {
        let cx = area
            .x
            .saturating_add(cur_col)
            .min(area.right().saturating_sub(1));
        let cy = area
            .y
            .saturating_add(cur_row)
            .min(area.bottom().saturating_sub(1));
        f.set_cursor_position((cx, cy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::widgets::input::InputConfig;

    #[test]
    fn single_line_visible_maps_cursor() {
        let mut input = Input::single_line(InputConfig::new());
        input.set_text("hello");
        let (lines, cursor) = visible(&input, 20, 3);
        assert_eq!(lines, vec!["hello".to_string()]);
        assert_eq!(cursor, (0, 5));
    }

    #[test]
    fn multi_line_wraps_long_line_and_maps_cursor() {
        let mut input = Input::multi_line(InputConfig::new());
        input.set_text("abcdef"); // width 4 → wraps to "abcd" / "ef"
        let (lines, cursor) = visible(&input, 4, 5);
        assert_eq!(lines, vec!["abcd".to_string(), "ef".to_string()]);
        // Cursor sits after "ef" on the second visual row.
        assert_eq!(cursor, (1, 2));
    }

    #[test]
    fn empty_input_visible_is_one_blank_row() {
        let input = Input::multi_line(InputConfig::new());
        let (lines, cursor) = visible(&input, 10, 3);
        assert_eq!(lines, vec![String::new()]);
        assert_eq!(cursor, (0, 0));
    }
}
