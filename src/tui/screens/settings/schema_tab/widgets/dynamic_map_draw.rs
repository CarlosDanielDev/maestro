//! Rendering routines for [`super::DynamicMapWidget`]. Split from the
//! widget's state machine to keep each file ≤ 400 LOC per RUST-GUARDRAILS §1.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Tabs},
};

use crate::tui::theme::Theme;

use super::dynamic_map::{DynamicMapWidget, MapFocus};
use super::dynamic_map_breadcrumb::nested_breadcrumb;
use super::entry_state::EntryState;

/// Tab-chip highlight style. Full orange chip when the SubtabStrip is
/// the current focus level (so `[` / `]` will switch tabs); muted
/// underline-only chip when the focus has descended into an EntryField
/// (so the user can see WHICH level the chord targets). Mirrors the
/// pattern of dimming non-active tab strips elsewhere in the TUI
/// (#908 visual-contrast fix).
fn tab_highlight_style(theme: &Theme, subtabstrip_focused: bool) -> Style {
    if subtabstrip_focused {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::UNDERLINED)
    }
}

pub(super) fn draw(
    widget: &DynamicMapWidget,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    focused: bool,
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
        // Active entry gets a filled selection background ONLY when the
        // SubtabStrip is the current focus level (so `[` / `]` chord
        // targets this level). When focus has descended into an
        // EntryField, the chip dims to a muted underline so two nested
        // tab strips do not both shout for attention (#908 contrast).
        let subtabstrip_focused = matches!(widget.focus(), MapFocus::SubtabStrip);
        let tabs = Tabs::new(titles)
            .select(highlight_idx)
            .highlight_style(tab_highlight_style(theme, subtabstrip_focused));
        f.render_widget(tabs, chunks[0]);

        if let Some(entry) = widget.active_entry() {
            let row_heights = widget.active_entry_row_heights();
            draw_entry_fields(f, chunks[1], theme, entry, widget.focus(), &row_heights);
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
        sf.widget.draw(f, rows[row_idx], theme, focused, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::FieldSchema;

    const EMPTY_FIELDS: &[FieldSchema] = &[];

    fn entry(id: &str) -> EntryState {
        EntryState::build("agents", id.to_string(), EMPTY_FIELDS, None)
    }

    fn title_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn tab_highlight_full_chip_when_subtabstrip_focused() {
        // #908 contrast — focus on SubtabStrip keeps the full orange
        // chip so the active tab reads as the chord target.
        let theme = Theme::dark();
        let style = tab_highlight_style(&theme, true);
        assert_eq!(
            style.bg,
            Some(theme.selection_bg),
            "must paint selection bg"
        );
        assert_eq!(
            style.fg,
            Some(theme.selection_fg),
            "must paint selection fg"
        );
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "must be BOLD when SubtabStrip is focused"
        );
    }

    #[test]
    fn tab_highlight_dim_underline_when_focus_descended_to_entry_field() {
        // #908 contrast — once focus descends into an EntryField the
        // chip drops the orange bg + bold so a second nested tab strip
        // is the only "shouting" chip on screen.
        let theme = Theme::dark();
        let style = tab_highlight_style(&theme, false);
        assert_eq!(style.bg, None, "must NOT paint bg when not focused");
        assert_eq!(
            style.fg,
            Some(theme.text_secondary),
            "must use text_secondary fg when not focused"
        );
        assert!(
            !style.add_modifier.contains(Modifier::BOLD),
            "must NOT be BOLD when focus descended"
        );
        assert!(
            style.add_modifier.contains(Modifier::UNDERLINED),
            "must be UNDERLINED to keep the active-tab signal"
        );
    }

    #[test]
    fn truncated_titles_returns_empty_for_no_entries() {
        let (titles, idx) = truncated_titles(&[], 0, 80);
        assert!(titles.is_empty());
        assert_eq!(idx, 0);
    }

    #[test]
    fn truncated_titles_fits_all_when_under_budget() {
        let entries: Vec<EntryState> = (0..5).map(|i| entry(&format!("a{i}"))).collect();
        let (titles, idx) = truncated_titles(&entries, 2, 80);
        assert_eq!(titles.len(), 5, "all entries fit, no truncation");
        assert_eq!(idx, 2, "highlight index unchanged when all fit");
    }

    #[test]
    fn truncated_titles_active_first_truncates_right_only() {
        let entries: Vec<EntryState> = (0..12).map(|i| entry(&format!("agent-{i:02}"))).collect();
        let (titles, idx) = truncated_titles(&entries, 0, 40);
        assert_eq!(
            title_text(&titles[0]),
            "agent-00",
            "active-first must place the active entry at slot 0"
        );
        assert_eq!(
            title_text(titles.last().unwrap()),
            "…",
            "active-first must end with the trailing ellipsis"
        );
        assert_eq!(idx, 0);
    }

    #[test]
    fn truncated_titles_active_last_truncates_left_only() {
        let entries: Vec<EntryState> = (0..12).map(|i| entry(&format!("agent-{i:02}"))).collect();
        let (titles, idx) = truncated_titles(&entries, 11, 40);
        assert_eq!(
            title_text(&titles[0]),
            "…",
            "active-last must start with the leading ellipsis"
        );
        assert_eq!(
            title_text(titles.last().unwrap()),
            "agent-11",
            "active-last must place the active entry at the end"
        );
        assert_eq!(idx, titles.len() - 1);
    }

    #[test]
    fn truncated_titles_active_middle_truncates_both_sides() {
        let entries: Vec<EntryState> = (0..12).map(|i| entry(&format!("agent-{i:02}"))).collect();
        let (titles, _idx) = truncated_titles(&entries, 6, 40);
        assert_eq!(
            title_text(&titles[0]),
            "…",
            "middle-active must have leading ellipsis"
        );
        assert_eq!(
            title_text(titles.last().unwrap()),
            "…",
            "middle-active must have trailing ellipsis"
        );
    }
}
