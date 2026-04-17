use super::AdaptScreen;
use super::types::AdaptStep;
use crate::tui::icons::{self, IconId};
use crate::tui::screens::{draw_keybinds_bar, sanitize_for_terminal};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::spinner::spinner_frame;

pub fn draw_adapt_screen(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    match screen.step {
        AdaptStep::Configure => draw_configure(screen, f, chunks[0], theme),
        step if step.is_progress() => draw_progress(screen, f, chunks[0], theme),
        AdaptStep::Complete => draw_complete(screen, f, chunks[0], theme),
        AdaptStep::Failed => draw_failed(screen, f, chunks[0], theme),
        _ => {}
    }

    let bindings = match screen.step {
        AdaptStep::Configure => vec![
            ("Enter", "Start"),
            ("Space", "Toggle"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        step if step.is_progress() => vec![("Esc", "Cancel")],
        AdaptStep::Complete => vec![("j/k", "Scroll"), ("Esc", "Back")],
        AdaptStep::Failed => vec![("r", "Retry"), ("Esc", "Back")],
        _ => vec![],
    };
    draw_keybinds_bar(f, chunks[1], &bindings, theme);
}

fn draw_configure(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let block = theme
        .styled_block("Adapt Project", false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let fields = [
        ("Path", field_text(&screen.config.path)),
        ("Dry Run", bool_text(screen.config.dry_run)),
        ("Scan Only", bool_text(screen.config.scan_only)),
        ("No Issues", bool_text(screen.config.no_issues)),
        ("Model", field_text(&screen.config.model)),
    ];

    let field_height = 2u16;
    let max_fields = (inner.height / field_height) as usize;

    for (i, (label, value)) in fields.iter().enumerate().take(max_fields) {
        let y = inner.y + (i as u16) * field_height;
        if y >= inner.y + inner.height {
            break;
        }
        let field_area = Rect::new(inner.x, y, inner.width, field_height);
        let is_selected = i == screen.selected_field;

        let style = if is_selected {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        let marker = if is_selected {
            format!("{} ", icons::get(IconId::ChevronRight))
        } else {
            "  ".to_string()
        };
        let line = Line::from(vec![
            Span::styled(marker, style),
            Span::styled(format!("{}: ", label), style),
            Span::styled(value, Style::default().fg(theme.text_secondary)),
        ]);
        f.render_widget(Paragraph::new(line), field_area);
    }
}

fn draw_progress(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let block = theme
        .styled_block("Adapt Pipeline", false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut phases = vec![(AdaptStep::Scanning, "Scanning project")];
    if !screen.config.scan_only {
        phases.push((AdaptStep::Analyzing, "Analyzing with Claude"));
    }
    if !screen.config.scan_only && !screen.config.no_issues {
        phases.push((AdaptStep::Consolidating, "Generating PRD"));
        phases.push((AdaptStep::Planning, "Generating plan"));
    }
    if !screen.config.scan_only && !screen.config.no_issues && !screen.config.dry_run {
        phases.push((AdaptStep::Scaffolding, "Scaffolding .claude/"));
        phases.push((AdaptStep::Materializing, "Creating issues"));
    }

    let current_idx = phases
        .iter()
        .position(|(step, _)| *step == screen.step)
        .unwrap_or(0);

    let mut lines = Vec::new();
    for (i, (_, label)) in phases.iter().enumerate() {
        let (marker, style) = if i < current_idx {
            // Completed
            let info = phase_summary(screen, phases[i].0);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", icons::get(IconId::CheckCircle)),
                    Style::default().fg(theme.accent_success),
                ),
                Span::styled(
                    *label,
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(info, Style::default().fg(theme.text_secondary)),
            ]));
            continue;
        } else if i == current_idx {
            // Active
            let spinner = spinner_frame(screen.spinner_tick);
            (
                format!("  {} ", spinner),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            // Pending
            (
                format!("  {} ", icons::get(IconId::Hourglass)),
                Style::default().fg(theme.text_secondary),
            )
        };
        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(*label, style),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn draw_complete(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let block = theme
        .styled_block("Adapt Complete", false)
        .border_style(Style::default().fg(theme.accent_success));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Pipeline completed successfully!",
        Style::default()
            .fg(theme.accent_success)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if let Some(ref profile) = screen.results.profile {
        lines.push(Line::from(vec![
            Span::styled("Language: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{:?}", profile.language),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Files: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{}", profile.source_stats.total_files),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Lines: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{}", profile.source_stats.total_lines),
                Style::default().fg(theme.text_primary),
            ),
        ]));
    }

    if let Some(ref report) = screen.results.report {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Modules: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{}", report.modules.len()),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Tech debt: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{}", report.tech_debt_items.len()),
                Style::default().fg(theme.text_primary),
            ),
        ]));
    }

    if let Some(ref plan) = screen.results.plan {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Milestones: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{}", plan.milestones.len()),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        let issue_count: usize = plan.milestones.iter().map(|m| m.issues.len()).sum();
        lines.push(Line::from(vec![
            Span::styled("Issues: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{}", issue_count),
                Style::default().fg(theme.text_primary),
            ),
        ]));
    }

    if let Some(ref scaffold) = screen.results.scaffold {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Scaffolded: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!(
                    "{} created, {} skipped",
                    scaffold.created_count, scaffold.skipped_count
                ),
                Style::default().fg(theme.text_primary),
            ),
        ]));
    }

    if let Some(ref mat) = screen.results.materialize {
        lines.push(Line::from(""));
        let label = if mat.dry_run { "Dry run" } else { "Created" };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", label),
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format!(
                    "{} milestones, {} issues",
                    mat.milestones_created.len(),
                    mat.issues_created.len()
                ),
                Style::default().fg(theme.text_primary),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines)
        .scroll((screen.scroll_offset, 0))
        .alignment(Alignment::Left);
    f.render_widget(paragraph, inner);
}

fn draw_failed(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let block = theme
        .styled_block("Adapt Failed", false)
        .border_style(Style::default().fg(theme.accent_error));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    if let Some(ref error) = screen.error {
        lines.push(Line::from(Span::styled(
            format!("Failed during: {:?}", error.phase),
            Style::default()
                .fg(theme.accent_error)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            sanitize_for_terminal(&error.message),
            Style::default().fg(theme.text_primary),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn field_text(s: &str) -> String {
    if s.is_empty() {
        "(empty)".to_string()
    } else {
        s.to_string()
    }
}

fn bool_text(v: bool) -> String {
    if v {
        "[x]".to_string()
    } else {
        "[ ]".to_string()
    }
}

fn phase_summary(screen: &AdaptScreen, step: AdaptStep) -> String {
    match step {
        AdaptStep::Scanning => {
            if let Some(ref p) = screen.results.profile {
                format!(" — {:?}, {} files", p.language, p.source_stats.total_files)
            } else {
                String::new()
            }
        }
        AdaptStep::Analyzing => {
            if let Some(ref r) = screen.results.report {
                format!(
                    " — {} modules, {} debt items",
                    r.modules.len(),
                    r.tech_debt_items.len()
                )
            } else {
                String::new()
            }
        }
        AdaptStep::Consolidating => {
            if screen.results.prd_content.is_some() {
                " — PRD generated".to_string()
            } else {
                String::new()
            }
        }
        AdaptStep::Planning => {
            if let Some(ref p) = screen.results.plan {
                let issues: usize = p.milestones.iter().map(|m| m.issues.len()).sum();
                format!(" — {} milestones, {} issues", p.milestones.len(), issues)
            } else {
                String::new()
            }
        }
        AdaptStep::Scaffolding => {
            if let Some(ref s) = screen.results.scaffold {
                format!(
                    " — {} created, {} skipped",
                    s.created_count, s.skipped_count
                )
            } else {
                String::new()
            }
        }
        AdaptStep::Materializing => {
            if let Some(ref m) = screen.results.materialize {
                format!(
                    " — {} milestones, {} issues created",
                    m.milestones_created.len(),
                    m.issues_created.len()
                )
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}
