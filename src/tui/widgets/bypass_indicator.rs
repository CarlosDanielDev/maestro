//! Persistent header badge shown when bypass mode is active (#328).

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #328. The header drawer in `app.rs` calls
// `draw()` when bypass is active in Phase 2; tests cover render today.
#![allow(dead_code)]

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassIndicatorState {
    Active,
    Inactive,
}

pub const BYPASS_BANNER: &str = " ⚠ BYPASS MODE — auto-accepting review corrections ";

pub fn draw(f: &mut Frame, area: Rect, state: BypassIndicatorState) {
    let line = render_line(state);
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_line(state: BypassIndicatorState) -> Line<'static> {
    match state {
        BypassIndicatorState::Active => Line::from(Span::styled(
            BYPASS_BANNER,
            Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        BypassIndicatorState::Inactive => Line::from(Span::raw("")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_line_uses_red_background() {
        let line = render_line(BypassIndicatorState::Active);
        let span = line
            .spans
            .first()
            .expect("active state must produce a span");
        assert_eq!(span.style.bg, Some(Color::Red));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(span.content.contains("BYPASS MODE"));
    }

    #[test]
    fn inactive_line_is_empty() {
        let line = render_line(BypassIndicatorState::Inactive);
        let span = line.spans.first().expect("expected one span");
        assert!(span.content.is_empty());
    }

    #[test]
    fn bypass_banner_contains_warning_glyph() {
        assert!(BYPASS_BANNER.contains("⚠"));
        assert!(BYPASS_BANNER.contains("BYPASS"));
    }

    #[test]
    fn render_does_not_panic_on_zero_area() {
        use ratatui::widgets::Widget;
        let mut buf = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 1, 1));
        let line = render_line(BypassIndicatorState::Active);
        // Widgets should be safe to render in extremely small viewports.
        ratatui::widgets::Paragraph::new(line).render(Rect::new(0, 0, 0, 0), &mut buf);
    }
}
