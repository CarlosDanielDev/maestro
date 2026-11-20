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
///
/// A label wider than `area` wraps at word boundaries onto the rows below,
/// under the label's first column (#1399); size the row with
/// [`checkbox_rows`].
pub fn draw_checkbox(
    f: &mut Frame,
    area: Rect,
    checked: bool,
    focused: bool,
    label: &str,
    theme: &Theme,
) {
    let indicator = checkbox_indicator(checked);
    let prefix = format!("  {} ", indicator);
    let rows = wrap_label(label, prefix.width(), area.width as usize);

    if focused {
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let lead = if i == 0 {
                    prefix.clone()
                } else {
                    " ".repeat(prefix.width())
                };
                Line::from(format!("{lead}{row}"))
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines).style(focused_selection_style(theme)),
            area,
        );
        return;
    }

    let check_color = if checked {
        theme.accent_success
    } else {
        theme.text_muted
    };
    let label_style = Style::default().fg(if checked {
        theme.text_primary
    } else {
        theme.text_secondary
    });

    let lines: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let lead = if i == 0 {
                Span::styled(
                    prefix.clone(),
                    Style::default()
                        .fg(check_color)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(" ".repeat(prefix.width()))
            };
            Line::from(vec![lead, Span::styled(row, label_style)])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}

fn checkbox_indicator(checked: bool) -> &'static str {
    if icons::use_nerd_font() {
        if checked {
            "\u{f046}" // nf-fa-check_square
        } else {
            "\u{f096}" // nf-fa-square_o
        }
    } else if checked {
        "[x]"
    } else {
        "[ ]"
    }
}

/// Greedy word wrap of `label` into rows of `width - indent` columns. A
/// word wider than a row stays whole on its own row (the area clips it).
fn wrap_label(label: &str, indent: usize, width: usize) -> Vec<String> {
    let room = width.saturating_sub(indent).max(1);
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in label.split(' ').filter(|w| !w.is_empty()) {
        if row.is_empty() {
            row.push_str(word);
        } else if row.width() + 1 + word.width() <= room {
            row.push(' ');
            row.push_str(word);
        } else {
            rows.push(std::mem::replace(&mut row, word.to_string()));
        }
    }
    rows.push(row);
    rows
}

/// Rows [`draw_checkbox`] needs to show `label` whole in `width` columns.
pub fn checkbox_rows(label: &str, checked: bool, width: u16) -> u16 {
    let indent = format!("  {} ", checkbox_indicator(checked)).width();
    wrap_label(label, indent, width as usize).len() as u16
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
