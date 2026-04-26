//! PRD screen rendering (#321).

#![deny(clippy::unwrap_used)]

use crate::prd::model::{Prd, TimelineStatus};
use crate::tui::screens::prd::chips::{save_chip, sync_chip};
use crate::tui::screens::prd::state::{EditTarget, PrdScreen, PrdSection};
use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

const HINT_GOALS: &str = "(no goals yet — press [n] to add one)";
const HINT_NON_GOALS: &str = "(no non-goals yet — press [n] to add one)";
const HINT_VISION: &str =
    "(no vision yet — synced from your PRD issue on GitHub, or press [y] to refresh)";
const HINT_STAKEHOLDERS: &str = "(none yet — edit prd.toml to add stakeholders)";
const HINT_TIMELINE: &str = "(no milestones yet — press [y] to sync from GitHub)";

pub fn draw(f: &mut Frame, area: Rect, screen: &PrdScreen, prd: &Prd, theme: &Theme) {
    let intro_height: u16 = if screen.first_view { 3 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(intro_height), // welcome banner (first view)
            Constraint::Length(3),            // header (status + counts)
            Constraint::Min(10),              // body
            Constraint::Length(3),            // hints
        ])
        .split(area);

    if screen.first_view {
        draw_intro(f, chunks[0], theme);
    }
    draw_header(f, chunks[1], screen, prd, theme);
    draw_body(f, chunks[2], screen, prd, theme);
    draw_hints(f, chunks[3], screen, theme);
}

fn draw_intro(f: &mut Frame, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_info))
        .title(" 👋 Welcome to your PRD ");
    let line = Line::from(Span::styled(
        "Live document for your project. [Tab] cycles sections, [n] adds items, [y] syncs from GitHub, [s] saves, [Esc] exits.",
        Style::default().fg(theme.text_secondary),
    ));
    f.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_header(f: &mut Frame, area: Rect, screen: &PrdScreen, prd: &Prd, theme: &Theme) {
    let dirty_marker = if screen.dirty { "*" } else { "" };
    let title = format!(" PRD{} — focus: {} ", dirty_marker, screen.focus.label());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(title);

    let mut spans = Vec::new();
    spans.push(Span::styled(
        format!(
            "Vision: {} chars • Goals: {} • Non-Goals: {} • Stakeholders: {} • Timeline: {}   ",
            prd.vision.len(),
            prd.goals.len(),
            prd.non_goals.len(),
            prd.stakeholders.len(),
            prd.timeline.len(),
        ),
        Style::default().fg(theme.text_secondary),
    ));
    if let Some(chip) = sync_chip(&screen.sync_status, theme) {
        spans.push(chip);
        spans.push(Span::raw("  "));
    }
    if let Some(chip) = save_chip(&screen.save_status, screen.dirty, theme) {
        spans.push(chip);
    }

    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn draw_body(f: &mut Frame, area: Rect, screen: &PrdScreen, prd: &Prd, theme: &Theme) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    draw_section_list(f, columns[0], screen, theme);
    draw_focused_section(f, columns[1], screen, prd, theme);
}

fn draw_section_list(f: &mut Frame, area: Rect, screen: &PrdScreen, theme: &Theme) {
    let items: Vec<ListItem> = PrdSection::ALL
        .iter()
        .map(|section| {
            let selected = *section == screen.focus;
            let style = if selected {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let prefix = if selected { "▶ " } else { "  " };
            ListItem::new(Line::from(Span::styled(
                format!("{prefix}{}", section.label()),
                style,
            )))
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_secondary))
        .title(" Sections ");
    f.render_widget(List::new(items).block(block), area);
}

fn draw_focused_section(f: &mut Frame, area: Rect, screen: &PrdScreen, prd: &Prd, theme: &Theme) {
    let body = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(format!(" {} ", screen.focus.label()));

    match screen.focus {
        PrdSection::Vision => draw_vision(f, area, body, prd, theme),
        PrdSection::Goals => draw_goals(f, area, body, screen, prd, theme),
        PrdSection::NonGoals => draw_non_goals(f, area, body, screen, prd, theme),
        PrdSection::CurrentState => draw_current_state(f, area, body, prd, theme),
        PrdSection::Stakeholders => draw_stakeholders(f, area, body, prd, theme),
        PrdSection::Timeline => draw_timeline(f, area, body, prd, theme),
    }
}

fn draw_vision(f: &mut Frame, area: Rect, block: Block<'_>, prd: &Prd, theme: &Theme) {
    let (text, color) = if prd.vision.trim().is_empty() {
        (HINT_VISION.to_string(), theme.text_secondary)
    } else {
        (prd.vision.clone(), theme.text_primary)
    };
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: true })
            .block(block),
        area,
    );
}

fn draw_goals(
    f: &mut Frame,
    area: Rect,
    block: Block<'_>,
    screen: &PrdScreen,
    prd: &Prd,
    theme: &Theme,
) {
    if prd.goals.is_empty() && screen.edit.is_none() {
        f.render_widget(
            Paragraph::new(HINT_GOALS)
                .style(Style::default().fg(theme.text_secondary))
                .block(block),
            area,
        );
        return;
    }

    let mut items: Vec<ListItem> = prd
        .goals
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let mark = if g.done { "[x]" } else { "[ ]" };
            let prefix = if i == screen.goal_cursor {
                "▶ "
            } else {
                "  "
            };
            let style = if i == screen.goal_cursor {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else if g.done {
                Style::default().fg(theme.text_secondary)
            } else {
                Style::default().fg(theme.text_primary)
            };
            ListItem::new(Line::from(Span::styled(
                format!("{prefix}{mark} {}", g.text),
                style,
            )))
        })
        .collect();

    if let Some(EditTarget::NewGoal { buffer }) = &screen.edit {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  + {buffer}_"),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ))));
    }

    f.render_widget(List::new(items).block(block), area);
}

fn draw_non_goals(
    f: &mut Frame,
    area: Rect,
    block: Block<'_>,
    screen: &PrdScreen,
    prd: &Prd,
    theme: &Theme,
) {
    if prd.non_goals.is_empty() && screen.edit.is_none() {
        f.render_widget(
            Paragraph::new(HINT_NON_GOALS)
                .style(Style::default().fg(theme.text_secondary))
                .block(block),
            area,
        );
        return;
    }

    let mut items: Vec<ListItem> = prd
        .non_goals
        .iter()
        .enumerate()
        .map(|(i, ng)| {
            let prefix = if i == screen.non_goal_cursor {
                "▶ "
            } else {
                "  "
            };
            let style = if i == screen.non_goal_cursor {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            ListItem::new(Line::from(Span::styled(format!("{prefix}- {ng}"), style)))
        })
        .collect();

    if let Some(EditTarget::NewNonGoal { buffer }) = &screen.edit {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  + {buffer}_"),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ))));
    }

    f.render_widget(List::new(items).block(block), area);
}

fn draw_current_state(f: &mut Frame, area: Rect, block: Block<'_>, prd: &Prd, theme: &Theme) {
    let cs = &prd.current_state;
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            "Issues: {} closed / {} total ({:.0}% complete)",
            cs.closed_issues,
            cs.total_issues(),
            cs.completion_ratio() * 100.0
        ),
        Style::default().fg(theme.text_primary),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "Milestones: {} closed / {} open",
            cs.closed_milestones, cs.open_milestones
        ),
        Style::default().fg(theme.text_primary),
    )));
    if !cs.top_blockers.is_empty() {
        let blockers = cs
            .top_blockers
            .iter()
            .map(|n| format!("#{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(Line::from(Span::styled(
            format!("Top blockers: {blockers}"),
            Style::default().fg(theme.accent_warning),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press [y] to refresh from GitHub",
        Style::default().fg(theme.text_secondary),
    )));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_stakeholders(f: &mut Frame, area: Rect, block: Block<'_>, prd: &Prd, theme: &Theme) {
    if prd.stakeholders.is_empty() {
        f.render_widget(
            Paragraph::new(HINT_STAKEHOLDERS)
                .style(Style::default().fg(theme.text_secondary))
                .block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = prd
        .stakeholders
        .iter()
        .map(|s| {
            ListItem::new(Line::from(Span::styled(
                format!("• {} — {}", s.name, s.role),
                Style::default().fg(theme.text_primary),
            )))
        })
        .collect();
    f.render_widget(List::new(items).block(block), area);
}

fn draw_timeline(f: &mut Frame, area: Rect, block: Block<'_>, prd: &Prd, theme: &Theme) {
    if prd.timeline.is_empty() {
        f.render_widget(
            Paragraph::new(HINT_TIMELINE)
                .style(Style::default().fg(theme.text_secondary))
                .block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = prd
        .timeline
        .iter()
        .map(|tm| {
            let target = tm
                .target_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unscheduled".into());
            let status_label = match tm.status {
                TimelineStatus::Planned => "planned",
                TimelineStatus::InProgress => "in-progress",
                TimelineStatus::Completed => "completed",
                TimelineStatus::Cancelled => "cancelled",
            };
            let color = match tm.status {
                TimelineStatus::Completed => theme.accent_success,
                TimelineStatus::InProgress => theme.accent_info,
                TimelineStatus::Cancelled => theme.text_secondary,
                TimelineStatus::Planned => theme.text_primary,
            };
            ListItem::new(Line::from(Span::styled(
                format!(
                    "• {} ({target}, {status_label}) — {:.0}%",
                    tm.name,
                    tm.progress * 100.0
                ),
                Style::default().fg(color),
            )))
        })
        .collect();
    f.render_widget(List::new(items).block(block), area);
}

fn draw_hints(f: &mut Frame, area: Rect, screen: &PrdScreen, theme: &Theme) {
    let hint = if screen.edit.is_some() {
        "[Enter] save  [Esc] cancel  type to fill"
    } else {
        match screen.focus {
            PrdSection::Goals => {
                "[↑↓] move  [n] new goal  [Space] done  [d] delete  [Tab] next  [s] save  [e] export  [y] sync  [o] explore  [R] reset  [Esc] back"
            }
            PrdSection::NonGoals => {
                "[↑↓] move  [n] new non-goal  [d] delete  [Tab] next  [s] save  [e] export  [y] sync  [o] explore  [R] reset  [Esc] back"
            }
            _ => "[Tab] next  [s] save  [e] export  [y] sync  [o] explore  [R] reset  [Esc] back",
        }
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
