//! Rendering routines for [`super::DynamicMapWidget`]. Split from the
//! widget's state machine to keep each file ≤ 400 LOC per RUST-GUARDRAILS §1.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Tabs},
};

use crate::tui::screens::settings::validation::ValidationFeedback;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

use super::dynamic_map::{DynamicMapWidget, MapFocus};
use super::dynamic_map_chrome::{nested_breadcrumb, tab_highlight_style};
use super::entry_state::EntryState;

pub(super) fn draw(
    widget: &DynamicMapWidget,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    focused: bool,
) {
    let empty: HashMap<String, ValidationFeedback> = HashMap::new();
    draw_with_warnings(widget, f, area, theme, focused, &empty);
}

/// Internal core for [`draw`] that threads a warnings-by-label lookup
/// through the nested per-entry field rows. The outer `draw` forwards
/// here with an empty map; the `SettingsScreen` render path passes a
/// populated lookup so inline `ValidationFeedback::warning` glyphs
/// appear next to offending sub-fields (#909).
pub(super) fn draw_with_warnings(
    widget: &DynamicMapWidget,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    focused: bool,
    warnings: &HashMap<String, ValidationFeedback>,
) {
    // Collapsed-row path: when the outer layout allocates a single line
    // (nested DynamicMap inside an unfocused entry field — e.g. the
    // teams tab's role_overrides slot), render a one-line summary so
    // the field's label is visible to the user. The expanded path
    // requires at least 2 rows (header + body).
    if area.height <= 1 {
        let count = widget.entries().len();
        let summary = if count == 0 {
            "(empty — focus to edit)".to_string()
        } else if count == 1 {
            "(1 entry — focus to edit)".to_string()
        } else {
            format!("({count} entries — focus to edit)")
        };
        let collapsed = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}: ", widget.label),
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(summary, Style::default().fg(theme.text_muted)),
        ]));
        f.render_widget(collapsed, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    let header_area = chunks[0];
    let inner = chunks[1];

    // #908 — when this widget is rendering as a nested editor (its
    // section_path carries the parent's dotted path, e.g.
    // `teams.<id>.role_overrides`) AND the outer field focus is on it,
    // replace the header label with a breadcrumb so users keep their
    // sense of place. Otherwise show the normal `<label>:` header.
    let active_role = widget.active_entry().map(|e| e.id.as_str());
    let header_line = if focused
        && let Some(crumbs) = nested_breadcrumb(&widget.section_path, active_role, area.width)
    {
        Line::from(Span::styled(
            crumbs,
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(
            format!("{}:", widget.label),
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        ))
    };
    f.render_widget(Paragraph::new(header_line), header_area);

    if widget.entries().is_empty() {
        let hint = Paragraph::new(vec![
            Line::from(Span::styled(
                "No entries yet.",
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[a] add first entry",
                Style::default().fg(theme.text_muted),
            )),
        ]);
        f.render_widget(hint, inner);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(inner);

        let (titles, highlight_idx) = truncated_titles(
            widget.entries(),
            widget.active_index().unwrap_or(0),
            chunks[0].width,
        );
        // Active entry gets a filled selection background ONLY when:
        // (a) the outer caller treats this widget as the chord target
        //     (`focused=true`) — so an unfocused widget on a tab where
        //     focus is on a sibling field does not shout for attention,
        //     AND
        // (b) the SubtabStrip itself is the current focus level — so
        //     `[` / `]` will switch tabs.
        // Otherwise the chip dims to a muted underline. Covers two
        // confusion cases reported on #908:
        //   1. Outer-tab strip stayed bright while focus had descended
        //      into a deeper field.
        //   2. DynamicMap chip stayed bright on a tab where focus was
        //      on a sibling widget (Providers tab "Default provider"
        //      dropdown made the `entries:` strip look like the chord
        //      target).
        let chord_target = focused && matches!(widget.focus(), MapFocus::SubtabStrip);
        let tabs = Tabs::new(titles)
            .select(highlight_idx)
            .highlight_style(tab_highlight_style(theme, chord_target));
        f.render_widget(tabs, chunks[0]);

        if let Some(entry) = widget.active_entry() {
            let row_heights = widget.active_entry_row_heights_for(warnings);
            draw_entry_fields(
                f,
                chunks[1],
                theme,
                entry,
                widget.focus(),
                &row_heights,
                warnings,
            );
        }
    }

    if widget.undo_active()
        && let Some(label) = widget.undo_label()
    {
        let banner_y = area.y + area.height.saturating_sub(1);
        let banner_area = Rect::new(area.x, banner_y, area.width, 1);
        let banner = Paragraph::new(Line::from(vec![Span::styled(
            format!("Removed '{}' — [u] to undo", label),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        )]));
        f.render_widget(banner, banner_area);
    }

    if let Some(modal) = widget.add_modal() {
        modal.draw(f, area, theme);
    }
    if let Some(modal) = widget.remove_modal() {
        modal.draw(f, area, theme);
    }
}

/// Build a Tabs strip slice that fits in `available_width`, centering the
/// window on `active_idx` and prepending/appending `…` when entries fall
/// off either side. Returns (titles, adjusted_highlight_idx).
///
/// Per spec §8 open question A (line 556–559): truncate-with-`…` is the
/// chosen overflow behaviour for v0.29.0; an overflow dropdown is deferred
/// to a follow-up (#792 acceptance criteria).
pub(super) fn truncated_titles<'a>(
    entries: &'a [EntryState],
    active_idx: usize,
    available_width: u16,
) -> (Vec<Line<'a>>, usize) {
    if entries.is_empty() {
        return (Vec::new(), 0);
    }
    // ratatui's Tabs draws " | " separator between entries.
    const SEP: u16 = 3;
    const ELLIPSIS_COL: u16 = 2; // "…" + leading or trailing space

    let widths: Vec<u16> = entries
        .iter()
        .map(|e| e.id.chars().count().clamp(1, 256) as u16)
        .collect();
    let total: u32 = widths
        .iter()
        .map(|w| u32::from(*w) + u32::from(SEP))
        .sum::<u32>()
        .saturating_sub(u32::from(SEP));

    if total <= u32::from(available_width) {
        let titles = entries.iter().map(|e| Line::from(e.id.as_str())).collect();
        return (titles, active_idx);
    }

    // Reserve budget for leading + trailing `… ` markers up front; if the
    // window naturally hits an edge we get the bytes back.
    let budget = available_width.saturating_sub(ELLIPSIS_COL * 2);
    let active = active_idx.min(entries.len() - 1);

    let mut start = active;
    let mut end = active;
    let mut used = widths[active];
    loop {
        let mut grew = false;
        if start > 0 {
            let candidate = used.saturating_add(widths[start - 1].saturating_add(SEP));
            if candidate <= budget {
                start -= 1;
                used = candidate;
                grew = true;
            }
        }
        if end + 1 < entries.len() {
            let candidate = used.saturating_add(widths[end + 1].saturating_add(SEP));
            if candidate <= budget {
                end += 1;
                used = candidate;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    let mut titles: Vec<Line<'a>> = Vec::new();
    if start > 0 {
        titles.push(Line::from("…"));
    }
    for entry in &entries[start..=end] {
        titles.push(Line::from(entry.id.as_str()));
    }
    if end + 1 < entries.len() {
        titles.push(Line::from("…"));
    }
    let highlight = active - start + usize::from(start > 0);
    (titles, highlight)
}

fn draw_entry_fields(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    entry: &EntryState,
    focus: &MapFocus,
    row_heights: &[(usize, u16)],
    warnings: &HashMap<String, ValidationFeedback>,
) {
    if row_heights.is_empty() {
        return;
    }
    let constraints: Vec<Constraint> = row_heights
        .iter()
        .map(|(_, h)| Constraint::Length(*h))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (row_idx, &(field_idx, _h)) in row_heights.iter().enumerate() {
        let Some(sf) = entry.fields.get(field_idx) else {
            continue;
        };
        let focused = matches!(focus, MapFocus::EntryField(n) if *n == field_idx);
        match &sf.widget {
            // Nested DynamicMap (the role_overrides editor inside a
            // teams entry): pass the same map down so its sub-fields
            // can look up their own warnings by fully-qualified label.
            WidgetKind::DynamicMap(inner) => {
                inner.draw_with_warnings(f, rows[row_idx], theme, focused, warnings);
            }
            _ => {
                let validation = warnings.get(sf.widget.label());
                sf.widget.draw(f, rows[row_idx], theme, focused, validation);
            }
        }
    }
}
