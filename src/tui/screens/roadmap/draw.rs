//! Roadmap screen rendering (#329).

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::tui::panels::compact_gauge_bar_counts;
use crate::tui::screens::roadmap::dep_levels::dep_levels;
use crate::tui::screens::roadmap::state::RoadmapScreen;
use crate::tui::screens::roadmap::types::{Filters, RoadmapEntry, StatusFilter};
use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

pub fn draw(f: &mut Frame, area: Rect, screen: &RoadmapScreen, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / filters
            Constraint::Min(10),   // body
            Constraint::Length(3), // hints
        ])
        .split(area);

    draw_header(f, chunks[0], screen, theme);
    draw_body(f, chunks[1], screen, theme);
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

fn draw_body(f: &mut Frame, area: Rect, screen: &RoadmapScreen, theme: &Theme) {
    if screen.entries.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.text_secondary));
        f.render_widget(
            Paragraph::new("No milestones loaded — press [r] to refresh from GitHub.")
                .style(Style::default().fg(theme.text_secondary))
                .block(block),
            area,
        );
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();
    for (idx, entry) in screen.entries.iter().enumerate() {
        let is_focused = idx == screen.cursor;
        items.push(milestone_row(entry, is_focused, &screen.filters, theme));
        if screen.is_expanded(entry.milestone.number) {
            for line in expanded_issue_lines(entry, &screen.filters, theme) {
                items.push(line);
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(" Milestones (descending semver) ");
    f.render_widget(List::new(items).block(block), area);
}

fn milestone_row<'a>(
    entry: &'a RoadmapEntry,
    focused: bool,
    filters: &Filters,
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
    let prefix = if focused { "▶ " } else { "  " };
    let filter_marker = if filters.is_empty() {
        ""
    } else {
        " (filtered)"
    };
    let mut spans = Vec::new();
    spans.push(Span::styled(
        prefix,
        Style::default().fg(theme.text_primary),
    ));
    spans.push(Span::styled(
        format!("{} ", entry.milestone.title),
        Style::default()
            .fg(if focused {
                theme.accent_success
            } else {
                theme.text_primary
            })
            .add_modifier(if focused {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    ));
    spans.push(Span::styled(
        format!("{bar} "),
        Style::default().fg(status_color),
    ));
    spans.push(Span::styled(
        format!("{}/{}{filter_marker}", entry.milestone.closed_issues, total),
        Style::default().fg(theme.text_secondary),
    ));
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
        "[↑↓] move  [Enter] expand/collapse  [r] refresh  [/] filter label  [a] filter assignee  [o] open-only  [c] closed-only  [x] clear filters  [Esc] back"
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
