//! The `▲ BYPASS` chip shown when bypass mode is active (#328).
//! `ds::status_bar` pins it on the right of the status row (#1257); it is
//! never drawn as a separate banner.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassIndicatorState {
    Active,
    Inactive,
}

/// Reverse warn colour plus bold, so it reads under NO_COLOR too.
pub const BYPASS_CHIP: &str = " ▲ BYPASS ";

pub fn render_line(state: BypassIndicatorState, theme: &Theme) -> Line<'static> {
    match state {
        BypassIndicatorState::Active => Line::from(Span::styled(
            BYPASS_CHIP,
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
        )),
        BypassIndicatorState::Inactive => Line::from(Span::raw("")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_line_is_reverse_warn_chip() {
        let theme = Theme::dark();
        let line = render_line(BypassIndicatorState::Active, &theme);
        let span = line
            .spans
            .first()
            .expect("active state must produce a span");
        assert_eq!(span.style.fg, Some(theme.accent_warning));
        assert!(span.style.add_modifier.contains(Modifier::REVERSED));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(span.content, BYPASS_CHIP);
    }

    #[test]
    fn inactive_line_is_empty() {
        let line = render_line(BypassIndicatorState::Inactive, &Theme::dark());
        let span = line.spans.first().expect("expected one span");
        assert!(span.content.is_empty());
    }

    #[test]
    fn bypass_chip_text() {
        assert!(BYPASS_CHIP.contains("▲ BYPASS"));
    }
}
