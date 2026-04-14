//! Shared "Unified PR" checkbox render helper.
//!
//! Used by both the issue browser overlay (#302) and prompt composition screen (#303).
//! This is a stateless render function — each screen owns its toggle state and keybinding.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::icons;
use crate::tui::theme::Theme;

/// Render a unified PR checkbox at the given area.
pub fn draw_unified_pr_toggle(f: &mut Frame, area: Rect, checked: bool, theme: &Theme) {
    let indicator = if icons::use_nerd_font() {
        if checked {
            "\u{f046}" // nf-fa-check_square
        } else {
            "\u{f096}" // nf-fa-square_o
        }
    } else if checked {
        "[x]"
    } else {
        "[ ]"
    };

    let check_color = if checked {
        theme.accent_success
    } else {
        theme.text_muted
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", indicator),
            Style::default()
                .fg(check_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Unified PR (single branch, closes all issues)",
            Style::default().fg(if checked {
                theme.text_primary
            } else {
                theme.text_secondary
            }),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
