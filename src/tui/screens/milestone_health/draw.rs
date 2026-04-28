//! Per-step rendering for the milestone-health wizard (#500).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::tui::screens::milestone_health::MilestoneHealthScreen;
use crate::tui::screens::milestone_health::diff::DiffLine;
use crate::tui::screens::milestone_health::format::{anomaly as format_anomaly, missing_fields};
use crate::tui::screens::milestone_health::state::{HealthStep, PatchOutcome};
use crate::tui::screens::sanitize_for_terminal as san;
use crate::tui::theme::Theme;

const BEFORE_AFTER_HEADER: &str = "Before / After";

pub fn draw(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    screen: &MilestoneHealthScreen,
    _spinner_tick: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);

    match &screen.state.step {
        HealthStep::Picker {
            milestones,
            selected,
        } => draw_picker(f, chunks[0], theme, milestones, *selected),
        HealthStep::Loading { label, .. } => draw_loading(f, chunks[0], theme, label),
        HealthStep::Empty { milestone } => draw_simple_message(
            f,
            chunks[0],
            theme,
            &format!(
                "No open issues to review for milestone '{}'.",
                san(&milestone.title)
            ),
            "Press any key to return to the picker.",
        ),
        HealthStep::Healthy { milestone } => {
            let summary = screen
                .state
                .report
                .as_ref()
                .map(|r| r.summary_line())
                .unwrap_or_default();
            draw_simple_message(
                f,
                chunks[0],
                theme,
                &format!(
                    "Milestone '{}' is healthy. No changes needed.",
                    san(&milestone.title)
                ),
                &summary,
            )
        }
        HealthStep::Report { milestone, .. } => draw_report(f, chunks[0], theme, milestone, screen),
        HealthStep::Patch {
            milestone, diff, ..
        } => draw_patch(f, chunks[0], theme, milestone, diff, screen.scroll),
        HealthStep::Confirm { milestone, .. } => draw_confirm(f, chunks[0], theme, milestone),
        HealthStep::Writing { milestone, .. } => draw_simple_message(
            f,
            chunks[0],
            theme,
            &format!("Updating milestone '{}'…", san(&milestone.title)),
            "Writing in progress — please wait.",
        ),
        HealthStep::Result { milestone, outcome } => {
            draw_result(f, chunks[0], theme, milestone, outcome)
        }
        HealthStep::FetchError { message } => draw_simple_message(
            f,
            chunks[0],
            theme,
            "Failed to fetch milestones from GitHub.",
            message,
        ),
    }

    draw_footer(f, chunks[1], theme, &screen.state.step);
}

fn draw_picker(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    milestones: &[crate::provider::github::types::GhMilestone],
    selected: usize,
) {
    let items: Vec<ListItem> = milestones
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let prefix = if i == selected { "▶ " } else { "  " };
            let label = format!(
                "{}#{} {} ({} open / {} closed)",
                prefix,
                m.number,
                san(&m.title),
                m.open_issues,
                m.closed_issues
            );
            let style = if i == selected {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Milestone Review — pick a milestone ");
    if items.is_empty() {
        let para = Paragraph::new("(no open milestones)").block(block);
        f.render_widget(para, area);
    } else {
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}

fn draw_loading(f: &mut Frame, area: Rect, _theme: &Theme, label: &str) {
    let para = Paragraph::new(label).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Milestone Review "),
    );
    f.render_widget(para, area);
}

fn draw_simple_message(f: &mut Frame, area: Rect, _theme: &Theme, title_msg: &str, hint: &str) {
    let lines = vec![
        Line::from(Span::raw(title_msg.to_string())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            hint.to_string(),
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];
    let para = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Milestone Review "),
    );
    f.render_widget(para, area);
}

fn draw_report(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    milestone: &crate::provider::github::types::GhMilestone,
    screen: &MilestoneHealthScreen,
) {
    let mut lines: Vec<Line> = Vec::new();

    if let Some(report) = screen.state.report.as_ref() {
        lines.push(Line::from(Span::styled(
            report.summary_line(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::raw("")));

        for r in &report.dor {
            let (label_text, color) = if r.passed() {
                ("PASS", theme.accent_success)
            } else {
                ("FAIL", theme.accent_warning)
            };
            let suffix = if r.passed() {
                String::new()
            } else {
                format!("missing: {}", missing_fields(&r.missing))
            };
            lines.push(Line::from(vec![
                Span::styled(label_text, Style::default().fg(color)),
                Span::raw(format!("  #{} ", r.issue_number)),
                Span::raw(suffix),
            ]));
        }

        if !report.anomalies.is_empty() {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled(
                "Graph anomalies:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for a in &report.anomalies {
                lines.push(Line::from(format_anomaly(a)));
            }
        }
    }

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((screen.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Health Report — {} ", san(&milestone.title))),
        );
    f.render_widget(para, area);
}

fn draw_patch(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    milestone: &crate::provider::github::types::GhMilestone,
    diff: &[DiffLine],
    scroll: u16,
) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        BEFORE_AFTER_HEADER.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for d in diff {
        match d {
            DiffLine::Same(s) => {
                lines.push(Line::from(Span::raw(format!("  {}", san(s)))));
            }
            DiffLine::Removed(s) => {
                lines.push(Line::from(Span::styled(
                    format!("- {}", san(s)),
                    Style::default().fg(theme.accent_warning),
                )));
            }
            DiffLine::Added(s) => {
                lines.push(Line::from(Span::styled(
                    format!("+ {}", san(s)),
                    Style::default().fg(theme.accent_success),
                )));
            }
        }
    }

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Patch Proposal — {} ", san(&milestone.title))),
        );
    f.render_widget(para, area);
}

fn draw_confirm(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    milestone: &crate::provider::github::types::GhMilestone,
) {
    let lines = vec![
        Line::from(Span::styled(
            "This will overwrite the milestone description on GitHub.",
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::raw(format!(
            "Milestone: #{} {}",
            milestone.number,
            san(&milestone.title)
        ))),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Press Enter to confirm.  Esc to abort.",
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Confirm "));
    f.render_widget(para, area);
}

fn draw_result(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    milestone: &crate::provider::github::types::GhMilestone,
    outcome: &PatchOutcome,
) {
    let (title, lines) = match outcome {
        PatchOutcome::Success => (
            " Updated ",
            vec![
                Line::from(Span::styled(
                    "Milestone description updated.",
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw(format!(
                    "Milestone: #{} {}",
                    milestone.number,
                    san(&milestone.title)
                ))),
            ],
        ),
        PatchOutcome::Error {
            message, retryable, ..
        } => (
            " Update failed ",
            vec![
                Line::from(Span::styled(
                    "Failed to update milestone description.",
                    Style::default()
                        .fg(theme.accent_warning)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw(message.clone())),
                Line::from(Span::styled(
                    if *retryable {
                        "Press r to retry, Esc to cancel."
                    } else {
                        "Press Esc to cancel."
                    }
                    .to_string(),
                    Style::default().add_modifier(Modifier::DIM),
                )),
            ],
        ),
    };
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, area);
}

fn draw_footer(f: &mut Frame, area: Rect, theme: &Theme, step: &HealthStep) {
    let hints: &[(&str, &str)] = match step {
        HealthStep::Picker { .. } => &[
            ("↑/↓", "select"),
            ("Enter", "review"),
            ("r", "refresh"),
            ("Esc", "back"),
        ],
        HealthStep::Loading { .. } => &[("Esc", "cancel")],
        HealthStep::Empty { .. } | HealthStep::Healthy { .. } => &[("any key", "back")],
        HealthStep::Report { .. } => {
            &[("Enter", "patch"), ("PgUp/PgDn", "scroll"), ("Esc", "back")]
        }
        HealthStep::Patch { .. } => &[
            ("Enter", "confirm"),
            ("PgUp/PgDn", "scroll"),
            ("Esc", "back"),
        ],
        HealthStep::Confirm { .. } => &[("Enter", "write to GitHub"), ("Esc", "abort")],
        HealthStep::Writing { .. } => &[("(writing…)", "")],
        HealthStep::Result {
            outcome: PatchOutcome::Error {
                retryable: true, ..
            },
            ..
        } => &[("r", "retry"), ("Esc", "back")],
        HealthStep::Result { .. } => &[("any key", "back")],
        HealthStep::FetchError { .. } => &[("any key", "retry")],
    };
    crate::tui::screens::draw_keybinds_bar(f, area, hints, theme);
}
