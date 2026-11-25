use super::AdaptScreen;
use super::types::AdaptStep;
use crate::tui::icons::{self, IconId};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::spinner::spinner_frame;

pub fn draw_adapt_screen(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    match screen.step {
        AdaptStep::Configure => super::draw_run::draw_configure(screen, f, area, theme),
        AdaptStep::Review => super::draw_run::draw_review(screen, f, area, theme),
        step if step.is_progress() => draw_progress(screen, f, area, theme),
        AdaptStep::Complete => draw_complete(screen, f, area, theme),
        AdaptStep::Failed => draw_failed(screen, f, area, theme),
        _ => {}
    }
}

/// Footer hints for the current step (#1258).
pub(super) fn footer_hints(screen: &AdaptScreen) -> Vec<(&'static str, &'static str)> {
    match screen.step {
        AdaptStep::Configure => vec![
            ("Enter", start_label(screen)),
            ("←→", "Stop after"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AdaptStep::Review => vec![
            ("Enter", "Arm · again writes"),
            ("j/k", "Scroll"),
            ("Esc", "Disarm · back"),
        ],
        AdaptStep::Scaffolding | AdaptStep::Materializing => {
            vec![("Esc", "Stop after this item")]
        }
        step if step.is_progress() => vec![("Esc", "Cancel")],
        AdaptStep::Complete => vec![("j/k", "Scroll"), ("Esc", "Back")],
        AdaptStep::Failed => vec![("r", "Retry"), ("Esc", "Back")],
        _ => vec![],
    }
}

/// The Configure ⏎ names the run it starts (§02.11 keymap).
fn start_label(screen: &AdaptScreen) -> &'static str {
    use super::StopAfter;
    if screen.loaded_from_cache {
        return "Resume";
    }
    match screen.config.stop_after {
        StopAfter::Scan => "Start scan",
        StopAfter::Analysis => "Start analysis",
        StopAfter::Plan => "Start dry run",
        StopAfter::Full => "Start full run",
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
        phases.push((
            AdaptStep::Review,
            "Pauses here: you review the plan before anything is written",
        ));
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

    if screen.is_stopping() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ‖ stopping after the item in flight · what was written is shown next",
            Style::default().fg(theme.accent_warning),
        )));
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
    // #1281: a run stopped by Esc says so over what it did write.
    let (banner, color) = if screen.is_stopping() {
        (
            "‖ Stopped by Esc: the items below were written, nothing after them",
            theme.accent_warning,
        )
    } else {
        ("Pipeline completed successfully!", theme.accent_success)
    };
    lines.push(Line::from(Span::styled(
        banner,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
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
            Span::styled(
                format!("{}s: ", screen.milestone_label()),
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format!("{}", plan.milestones.len()),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        let issue_count: usize = plan.milestones.iter().map(|m| m.issues.len()).sum();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", screen.issue_label_plural()),
                Style::default().fg(theme.text_secondary),
            ),
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
        for file in scaffold
            .files
            .iter()
            .filter(|f| f.status == crate::adapt::types::ScaffoldFileStatus::Created)
        {
            lines.push(Line::from(Span::styled(
                format!("  + {}", sanitize_for_terminal(&file.path)),
                Style::default().fg(theme.text_secondary),
            )));
        }
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
                    "{} {}s, {} {}",
                    mat.milestones_created.len(),
                    screen.milestone_label_lowercase(),
                    mat.issues_created.len(),
                    screen.issue_label_plural_lowercase()
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
                format!(
                    " — {} {}s, {} {}",
                    p.milestones.len(),
                    screen.milestone_label_lowercase(),
                    issues,
                    screen.issue_label_plural_lowercase()
                )
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
                    " — {} {}s, {} {} created",
                    m.milestones_created.len(),
                    screen.milestone_label_lowercase(),
                    m.issues_created.len(),
                    screen.issue_label_plural_lowercase()
                )
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}
