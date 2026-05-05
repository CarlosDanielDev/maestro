//! Roadmap screen rendering (#329).

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::tui::panels::compact_gauge_bar_counts;
use crate::tui::screens::roadmap::dep_levels::dep_levels;
use crate::tui::screens::roadmap::state::RoadmapScreen;
use crate::tui::screens::roadmap::types::{Filters, RoadmapEntry, StatusFilter};
use crate::tui::theme::Theme;
use crate::tui::widgets::{EmptyState, focused_selection_style};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use unicode_width::UnicodeWidthStr;

pub fn draw(
    f: &mut Frame,
    area: Rect,
    screen: &mut RoadmapScreen,
    theme: &Theme,
    spinner_tick: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / filters
            Constraint::Min(10),   // body
            Constraint::Length(3), // hints
        ])
        .split(area);

    draw_header(f, chunks[0], screen, theme);
    draw_body(f, chunks[1], screen, theme, spinner_tick);
    draw_hints(f, chunks[2], screen, theme);
}

fn draw_header(f: &mut Frame, area: Rect, screen: &RoadmapScreen, theme: &Theme) {
    let filters = &screen.filters;
    let filter_label = if filters.is_empty() {
        "no filters".to_string()
    } else {
        let mut parts = Vec::new();
        if !filters.label.is_empty() {
            parts.push(format!("label~{}", filters.label));
        }
        if !filters.assignee.is_empty() {
            parts.push(format!("assignee~{}", filters.assignee));
        }
        match filters.status {
            StatusFilter::Open => parts.push("open".into()),
            StatusFilter::Closed => parts.push("closed".into()),
            StatusFilter::Any => {}
        }
        parts.join(" • ")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(format!(
            " Roadmap ({} milestones, {filter_label}) ",
            screen.entries.len()
        ));
    let editing = screen
        .editing_filter
        .as_ref()
        .map(|f| format!("editing: {f:?}"))
        .unwrap_or_default();
    f.render_widget(
        Paragraph::new(editing)
            .style(Style::default().fg(theme.text_secondary))
            .block(block),
        area,
    );
}

fn draw_body(
    f: &mut Frame,
    area: Rect,
    screen: &mut RoadmapScreen,
    theme: &Theme,
    spinner_tick: usize,
) {
    if screen.entries.is_empty() {
        if screen.is_loading {
            EmptyState::loading("Roadmap", "Fetching milestones from GitHub…", spinner_tick)
                .render(f, area, theme);
        } else {
            EmptyState::idle(
                "Roadmap",
                "No milestones yet.",
                "Press [r] to refresh, [m] to create one.",
            )
            .render(f, area, theme);
        }
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(" Milestones (descending semver) ");
    let inner = block.inner(area);
    let visible_rows = inner.height as usize;
    screen.clamp_offset_to_cursor(visible_rows);
    let offset = screen.offset;

    let mut items: Vec<ListItem> = Vec::new();
    for (idx, entry) in screen.entries.iter().enumerate() {
        let is_focused = idx == screen.cursor;
        items.push(milestone_row(
            entry,
            is_focused,
            &screen.filters,
            inner.width,
            theme,
        ));
        if screen.is_expanded(entry.milestone.number) {
            for line in expanded_issue_lines(entry, &screen.filters, theme) {
                items.push(line);
            }
        }
    }

    let items = items.into_iter().skip(offset).collect::<Vec<_>>();
    f.render_widget(List::new(items).block(block), area);
}

fn milestone_row<'a>(
    entry: &'a RoadmapEntry,
    focused: bool,
    filters: &Filters,
    width: u16,
    theme: &Theme,
) -> ListItem<'a> {
    let total = entry.milestone.open_issues + entry.milestone.closed_issues;
    let pct = if total > 0 {
        f64::from(entry.milestone.closed_issues) / f64::from(total) * 100.0
    } else {
        0.0
    };
    let bar_width = 12_usize;
    let (filled, empty) = compact_gauge_bar_counts(pct, bar_width);
    let bar = format!("[{}{}]", "█".repeat(filled), "·".repeat(empty));
    let is_closed = entry.milestone.state.eq_ignore_ascii_case("closed");
    let all_done = total > 0 && entry.milestone.closed_issues == total;
    let halfway = total > 0 && entry.milestone.closed_issues * 2 >= total;
    let status_color = if is_closed || all_done {
        theme.accent_success
    } else if halfway {
        theme.accent_info
    } else {
        theme.accent_warning
    };
    let filter_marker = if filters.is_empty() {
        ""
    } else {
        " (filtered)"
    };
    let row_style = if focused {
        focused_selection_style(theme)
    } else {
        Style::default().fg(theme.text_primary)
    };
    let status_style = if focused {
        focused_selection_style(theme)
    } else {
        Style::default().fg(status_color)
    };
    let count_text = format!("{}/{}{filter_marker}", entry.milestone.closed_issues, total);
    let row_text = format!("  {} {bar} {count_text}", entry.milestone.title);
    let trailing = if focused {
        width.saturating_sub(row_text.width() as u16) as usize
    } else {
        0
    };
    let mut spans = Vec::new();
    spans.push(Span::styled("  ", row_style));
    spans.push(Span::styled(
        format!("{} ", entry.milestone.title),
        if focused {
            row_style
        } else {
            row_style.add_modifier(Modifier::BOLD)
        },
    ));
    spans.push(Span::styled(format!("{bar} "), status_style));
    spans.push(Span::styled(
        count_text,
        if focused {
            row_style
        } else {
            Style::default().fg(theme.text_secondary)
        },
    ));
    if trailing > 0 {
        spans.push(Span::styled(" ".repeat(trailing), row_style));
    }
    ListItem::new(Line::from(spans))
}

fn expanded_issue_lines<'a>(
    entry: &'a RoadmapEntry,
    filters: &Filters,
    theme: &Theme,
) -> Vec<ListItem<'a>> {
    let visible: Vec<_> = entry.issues.iter().filter(|i| filters.matches(i)).collect();

    if visible.is_empty() {
        return vec![ListItem::new(Line::from(Span::styled(
            "      (no matching issues)",
            Style::default().fg(theme.text_secondary),
        )))];
    }

    let mut inputs: Vec<(u64, Vec<u64>)> = Vec::with_capacity(visible.len());
    let visible_set: std::collections::HashSet<u64> = visible.iter().map(|i| i.number).collect();
    for issue in &visible {
        let blockers = issue
            .all_blockers()
            .into_iter()
            .filter(|b| visible_set.contains(b))
            .collect();
        inputs.push((issue.number, blockers));
    }

    let levels =
        dep_levels(&inputs).unwrap_or_else(|_| vec![visible.iter().map(|i| i.number).collect()]);

    let mut lines: Vec<ListItem> = Vec::new();
    for (level_idx, level) in levels.iter().enumerate() {
        lines.push(ListItem::new(Line::from(Span::styled(
            format!("    Level {level_idx}:"),
            Style::default().fg(theme.text_secondary),
        ))));
        for n in level {
            if let Some(issue) = visible.iter().find(|i| i.number == *n) {
                let status_color = if issue.state.eq_ignore_ascii_case("closed") {
                    theme.accent_success
                } else {
                    theme.text_primary
                };
                lines.push(ListItem::new(Line::from(Span::styled(
                    format!("      #{} {}", issue.number, issue.title),
                    Style::default().fg(status_color),
                ))));
            }
        }
    }
    lines
}

fn draw_hints(f: &mut Frame, area: Rect, screen: &RoadmapScreen, theme: &Theme) {
    let hint = if screen.editing_filter.is_some() {
        "[Enter] apply  [Esc] cancel  type to filter"
    } else {
        "[j/k] move  [Enter] expand/collapse  [r] refresh  [/] filter label  [a] filter assignee  [o] open-only  [c] closed-only  [x] clear filters  [Esc] back"
    };
    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(theme.text_secondary))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.text_secondary)),
            ),
        area,
    );
}
