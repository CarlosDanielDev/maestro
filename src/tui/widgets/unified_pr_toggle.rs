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
use unicode_width::UnicodeWidthStr;

use crate::tui::icons;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

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

/// Render a focus-aware labelled checkbox at the given area.
///
/// Sibling of [`draw_unified_pr_toggle`] for screens that own several
/// checkboxes (the issue-launch dialog, #733). When `focused`, the whole row
/// is painted with the standard selection bar (`focused_selection_style`),
/// matching the issue-list selected row.
pub fn draw_checkbox(
    f: &mut Frame,
    area: Rect,
    checked: bool,
    focused: bool,
    label: &str,
    theme: &Theme,
) {
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

    if focused {
        f.render_widget(
            focus_bar(&format!("  {} {}", indicator, label), area, theme),
            area,
        );
        return;
    }

    let check_color = if checked {
        theme.accent_success
    } else {
        theme.text_muted
    };

    let line = Line::from(vec![
        Span::styled(
            format!("  {} ", indicator),
            Style::default()
                .fg(check_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            label.to_string(),
            Style::default().fg(if checked {
                theme.text_primary
            } else {
                theme.text_secondary
            }),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

/// Build a full-width selection bar: `content` padded with spaces to `area`
/// width, styled with the shared `focused_selection_style`. Used to give a
/// focused row the same highlight as the selected issue-list row.
pub fn focus_bar<'a>(content: &str, area: Rect, theme: &Theme) -> Paragraph<'a> {
    let width = area.width as usize;
    let used = UnicodeWidthStr::width(content);
    let padded = format!("{}{}", content, " ".repeat(width.saturating_sub(used)));
    Paragraph::new(Line::from(Span::styled(
        padded,
        focused_selection_style(theme),
    )))
}
