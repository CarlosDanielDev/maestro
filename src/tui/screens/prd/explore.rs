//! Explore-PRD-sources panel (#321).
//!
//! When the user presses `[o]` on the PRD screen the right-hand pane
//! flips from the focused section to a list of every PRD candidate
//! discovered across GitHub + local + Azure. The user can pick one with
//! arrow keys + Enter to re-ingest from that specific source.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::prd::discover::DiscoveredPrd;
use crate::prd::ingest::{IngestedPrd, parse_markdown};
use crate::tui::theme::Theme;
use crate::util::formatting::truncate_at_char_boundary;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

const PREVIEW_RAW_LINES: usize = 12;
const PREVIEW_LIST_MAX: usize = 8;
const PREVIEW_ITEM_WIDTH: usize = 80;
const PREVIEW_RAW_WIDTH: usize = 70;

pub fn draw(
    f: &mut Frame,
    area: Rect,
    candidates: &[DiscoveredPrd],
    parsed_cache: &[IngestedPrd],
    cursor: usize,
    theme: &Theme,
) {
    if candidates.is_empty() {
        draw_empty(f, area, theme);
        return;
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    draw_candidate_list(f, columns[0], candidates, parsed_cache, cursor, theme);
    if let (Some(focused), Some(parsed)) = (candidates.get(cursor), parsed_cache.get(cursor)) {
        draw_preview(f, columns[1], focused, parsed, theme);
    }
}

fn draw_empty(f: &mut Frame, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_info))
        .title(" PRD Sources ");
    let msg = Paragraph::new(
        "No PRD candidates found. Tried: GitHub label `prd`, GitHub issue #1, local `docs/PRD.md`, Azure wiki `/PRD`, GitHub `PRD: in:title`.",
    )
    .style(Style::default().fg(theme.text_secondary))
    .wrap(Wrap { trim: true })
    .block(block);
    f.render_widget(msg, area);
}

fn draw_candidate_list(
    f: &mut Frame,
    area: Rect,
    candidates: &[DiscoveredPrd],
    parsed_cache: &[IngestedPrd],
    cursor: usize,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_info))
        .title(format!(" PRD Sources ({} found) ", candidates.len()));
    let items: Vec<ListItem> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let parsed = parsed_cache
                .get(i)
                .cloned()
                .unwrap_or_else(|| parse_markdown(&c.body));
            candidate_row(i, c, &parsed, cursor, theme)
        })
        .collect();
    f.render_widget(List::new(items).block(block), area);
}

fn candidate_row<'a>(
    i: usize,
    c: &'a DiscoveredPrd,
    parsed: &IngestedPrd,
    cursor: usize,
    theme: &Theme,
) -> ListItem<'a> {
    let focused = i == cursor;
    let prefix = if focused { "▶ " } else { "  " };
    let stats = format!(
        "v={} g={} ng={} s={}",
        if parsed.vision.is_some() { "✓" } else { "·" },
        parsed.goals.len(),
        parsed.non_goals.len(),
        parsed.stakeholders.len(),
    );
    let identifier = if c.number > 0 {
        format!("#{} {}", c.number, c.title)
    } else {
        c.title.clone()
    };
    ListItem::new(vec![
        Line::from(vec![
            Span::styled(prefix, Style::default().fg(theme.text_primary)),
            Span::styled(
                format!("[{}]", c.source.label()),
                Style::default().fg(theme.accent_info),
            ),
        ]),
        Line::from(Span::styled(
            format!("    {identifier}"),
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
        )),
        Line::from(Span::styled(
            format!("    {stats}"),
            Style::default().fg(theme.text_secondary),
        )),
    ])
}

fn draw_preview(f: &mut Frame, area: Rect, c: &DiscoveredPrd, parsed: &IngestedPrd, theme: &Theme) {
    let title = if c.number > 0 {
        format!(" Preview: #{} ({}) ", c.number, c.source.label())
    } else {
        format!(" Preview: {} ({}) ", c.title, c.source.label())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(title);

    let mut lines = build_preview_lines(c, parsed, theme);
    let total_lines = c.body.lines().count();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── Raw body excerpt ──",
        Style::default().fg(theme.text_secondary),
    )));
    for raw_line in c.body.lines().take(PREVIEW_RAW_LINES) {
        lines.push(Line::from(Span::styled(
            truncate_str(raw_line, PREVIEW_RAW_WIDTH).to_string(),
            Style::default().fg(theme.text_secondary),
        )));
    }
    if total_lines > PREVIEW_RAW_LINES {
        lines.push(Line::from(Span::styled(
            format!("… ({} more lines)", total_lines - PREVIEW_RAW_LINES),
            Style::default().fg(theme.text_secondary),
        )));
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn build_preview_lines<'a>(
    c: &'a DiscoveredPrd,
    parsed: &'a IngestedPrd,
    theme: &Theme,
) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Title: ", Style::default().fg(theme.text_secondary)),
        Span::styled(c.title.clone(), Style::default().fg(theme.text_primary)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Source: ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            c.source.label().to_string(),
            Style::default().fg(theme.accent_info),
        ),
    ]));
    if c.number > 0 {
        lines.push(Line::from(vec![
            Span::styled("Issue: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("#{}", c.number),
                Style::default().fg(theme.accent_identifier),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(section_header("Vision", theme));
    lines.push(Line::from(Span::styled(
        parsed
            .vision
            .clone()
            .unwrap_or_else(|| "(none extracted)".into()),
        Style::default().fg(theme.text_primary),
    )));
    lines.push(Line::from(""));
    lines.push(section_header(
        &format!("Goals ({})", parsed.goals.len()),
        theme,
    ));
    if parsed.goals.is_empty() {
        lines.push(Line::from(Span::styled(
            "(none extracted)",
            Style::default().fg(theme.text_secondary),
        )));
    } else {
        push_bullets(&mut lines, &parsed.goals, |g| g.clone(), theme);
    }
    lines.push(Line::from(""));
    lines.push(section_header(
        &format!("Non-Goals ({})", parsed.non_goals.len()),
        theme,
    ));
    if parsed.non_goals.is_empty() {
        lines.push(Line::from(Span::styled(
            "(none extracted)",
            Style::default().fg(theme.text_secondary),
        )));
    } else {
        push_bullets(&mut lines, &parsed.non_goals, |ng| ng.clone(), theme);
    }
    lines.push(Line::from(""));
    lines.push(section_header(
        &format!("Stakeholders ({})", parsed.stakeholders.len()),
        theme,
    ));
    if parsed.stakeholders.is_empty() {
        lines.push(Line::from(Span::styled(
            "(none extracted)",
            Style::default().fg(theme.text_secondary),
        )));
    } else {
        push_bullets(
            &mut lines,
            &parsed.stakeholders,
            |(name, role)| format!("{name} — {role}"),
            theme,
        );
    }
    lines
}

/// Render up to PREVIEW_LIST_MAX bullet items + a `… N more` overflow
/// line if the underlying slice is longer. Used by Goals / Non-Goals /
/// Stakeholders so they all share the same truncation rule.
fn push_bullets<'a, T, F>(lines: &mut Vec<Line<'a>>, items: &'a [T], fmt: F, theme: &Theme)
where
    F: Fn(&T) -> String,
{
    for it in items.iter().take(PREVIEW_LIST_MAX) {
        let text = fmt(it);
        let truncated = truncate_str(&text, PREVIEW_ITEM_WIDTH).to_string();
        lines.push(Line::from(Span::styled(
            format!("  • {truncated}"),
            Style::default().fg(theme.text_primary),
        )));
    }
    if items.len() > PREVIEW_LIST_MAX {
        lines.push(Line::from(Span::styled(
            format!("  … {} more", items.len() - PREVIEW_LIST_MAX),
            Style::default().fg(theme.text_secondary),
        )));
    }
}

fn section_header(name: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("── {name} ──"),
        Style::default()
            .fg(theme.accent_identifier)
            .add_modifier(Modifier::BOLD),
    ))
}

fn truncate_str(s: &str, max: usize) -> &str {
    let end = truncate_at_char_boundary(s, max);
    &s[..end]
}
