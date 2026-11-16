//! SizeGuard screen (design system §04 `ds::size_guard`): drawn instead of
//! any mode when the terminal is below [`MIN_SIZE`], so the user reads what
//! the minimum is rather than an overlapping layout.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Padding, Paragraph, Wrap},
};

use super::breakpoint::MIN_SIZE;
use super::panel::{Focus, panel};
use crate::tui::theme::Theme;

pub fn draw(f: &mut Frame, area: Rect, theme: &Theme) {
    let (min_w, min_h) = MIN_SIZE;
    let bold = Modifier::BOLD;
    let muted = Style::default().fg(theme.text_muted);
    let short = Style::default().fg(theme.accent_error).add_modifier(bold);
    let ok = Style::default().fg(theme.text_primary);
    let dimension = |label: &'static str, have: u16, need: u16| {
        let style = if have < need { short } else { ok };
        Line::from(vec![
            Span::styled(label, muted),
            Span::styled(format!("{have}/{need}"), style),
        ])
    };
    let lines = vec![
        Line::from(vec![
            Span::raw("maestro needs "),
            Span::styled(
                format!("{min_w} x {min_h}"),
                Style::default().fg(theme.border_active).add_modifier(bold),
            ),
            Span::raw(" - this window is "),
            Span::styled(format!("{} x {}", area.width, area.height), short),
        ]),
        Line::default(),
        dimension("width  ", area.width, min_w),
        dimension("height ", area.height, min_h),
        Line::default(),
        Line::styled(
            "Agents keep running in the background. Resize to continue.",
            muted,
        ),
    ];
    let block = panel("Terminal too small", Focus::Overlay, theme)
        .border_style(Style::default().fg(theme.accent_warning))
        .padding(Padding::horizontal(1));
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: true })
            .block(block),
        area,
    );
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn render(w: u16, h: u16, theme: &Theme) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, f.area(), theme)).unwrap();
        terminal
    }

    #[test]
    fn size_guard_states_minimum_and_current_size() {
        let out = format!("{}", render(72, 12, &Theme::dark()).backend());
        assert!(out.contains("Terminal too small"), "{out}");
        assert!(out.contains("80 x 24"), "minimum missing: {out}");
        assert!(out.contains("72 x 12"), "current size missing: {out}");
    }

    #[test]
    fn size_guard_does_not_panic_on_tiny_areas() {
        for (w, h) in [(1, 1), (2, 2), (10, 3), (1, 30)] {
            render(w, h, &Theme::dark());
        }
    }

    #[test]
    fn size_guard_no_color() {
        let terminal = render(60, 10, &Theme::no_color());
        let out = format!("{:?}", terminal.backend().buffer());
        assert!(out.contains("60 x 10"), "{out}");
        assert_snapshot!(out);
    }
}
