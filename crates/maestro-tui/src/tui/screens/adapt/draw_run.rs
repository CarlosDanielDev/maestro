//! #1281: the Configure form with the Stop after radio and the live
//! "This run will" plan, and the Review step before any write (§02.11).

use std::time::Instant;

use super::AdaptScreen;
use super::stop_after::{StopAfter, run_plan};
use crate::provider::types::provider_display_name;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::icons::{self, IconId};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::armed_bar::{ARMED_BAR_HEIGHT, ArmedBar};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn draw_configure(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let compact = Breakpoint::from_area(f.area()) == Breakpoint::Compact;
    let form_lines = form_lines(screen, theme, compact);
    let remote = provider_display_name(screen.provider_kind);
    let plan = run_plan(&screen.config, remote);
    let [form, plan_area] = if compact {
        // #1281: at 80×24 the form takes the rows it needs (every option
        // and Model stay visible) and the plan gets one line per heading.
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(form_lines.len() as u16 + 2),
                Constraint::Min(0),
            ])
            .areas(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area)
    };
    let block = panel("Adapt project", Focus::Active, theme);
    let inner = block.inner(form);
    f.render_widget(block, form);
    f.render_widget(Paragraph::new(form_lines), inner);

    let block = panel("This run will", Focus::Inactive, theme);
    let inner = block.inner(plan_area);
    f.render_widget(block, plan_area);
    let body = if compact {
        // One line per heading, wrapped so no fact is cut at 80 columns.
        Paragraph::new(compact_plan_lines(&plan, theme))
            .wrap(ratatui::widgets::Wrap { trim: false })
    } else {
        Paragraph::new(plan_lines(&plan, theme))
    };
    f.render_widget(body, inner);
}

/// Project · Stop after · Model. Compact drops the blank rows and shows
/// only the chosen option's consequence.
fn form_lines(screen: &AdaptScreen, theme: &Theme, compact: bool) -> Vec<Line<'static>> {
    let label = |i: usize, name: &str| {
        let selected = i == screen.selected_field;
        let style = if selected {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let marker = if selected {
            format!("{} ", icons::get(IconId::ChevronRight))
        } else {
            "  ".to_string()
        };
        vec![
            Span::styled(marker, style),
            Span::styled(format!("{name}: "), style),
        ]
    };
    let value = |s: &str| {
        let shown = if s.is_empty() { "(empty)" } else { s };
        Span::styled(
            sanitize_for_terminal(shown),
            Style::default().fg(theme.text_secondary),
        )
    };
    let dim = Style::default().fg(theme.text_secondary);

    let mut lines = Vec::new();
    let mut path = label(0, "Project");
    path.push(value(&screen.config.path));
    lines.push(Line::from(path));
    if !compact {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(label(1, "Stop after")));
    for option in StopAfter::ALL {
        let on = option == screen.config.stop_after;
        let color = if option == StopAfter::Full {
            theme.accent_warning
        } else {
            theme.text_primary
        };
        let mut style = Style::default().fg(color);
        if on {
            style = style.add_modifier(Modifier::BOLD);
        }
        lines.push(Line::from(vec![
            Span::styled(format!("    {} ", if on { "(•)" } else { "( )" }), style),
            Span::styled(option.label(), style),
        ]));
        if !compact {
            lines.push(Line::from(Span::styled(
                format!("        {}", option.consequence()),
                dim,
            )));
        }
    }
    if compact {
        lines.push(Line::from(Span::styled(
            format!("        {}", screen.config.stop_after.consequence()),
            dim,
        )));
    } else {
        lines.push(Line::from(""));
    }
    let mut model = label(2, "Model");
    model.push(value(&screen.config.model));
    lines.push(Line::from(model));
    if screen.loaded_from_cache {
        // `x` types on Project and Model, so the discard chord is offered
        // only on Stop after (#1281).
        let text = if screen.selected_field == 1 {
            "Cached run found: ⏎ resumes it · x x discards it"
        } else {
            "Cached run found: ⏎ resumes it"
        };
        lines.push(Line::from(Span::styled(
            text,
            Style::default().fg(theme.accent_warning),
        )));
    }
    lines
}

/// Headings drawn once, their lines under them.
fn plan_lines(plan: &[(&'static str, String)], theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut last = "";
    for (heading, line) in plan {
        if *heading != last {
            if !last.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                *heading,
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            )));
            last = heading;
        }
        lines.push(Line::from(Span::styled(
            format!("  {}", sanitize_for_terminal(line)),
            Style::default().fg(theme.text_primary),
        )));
    }
    lines
}

/// One line per heading: `READ  a · b`.
fn compact_plan_lines(plan: &[(&'static str, String)], theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut last = "";
    let mut text = String::new();
    let flush = |heading: &'static str, text: &str, lines: &mut Vec<Line<'static>>| {
        if !heading.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{heading}  "),
                    Style::default()
                        .fg(theme.text_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    sanitize_for_terminal(text),
                    Style::default().fg(theme.text_primary),
                ),
            ]));
        }
    };
    for (heading, line) in plan {
        if *heading != last {
            flush(last, &text, &mut lines);
            last = heading;
            text.clear();
        } else {
            text.push_str(" · ");
        }
        text.push_str(line);
    }
    flush(last, &text, &mut lines);
    lines
}

pub(super) fn draw_review(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let block = panel("Review · ○ nothing written yet", Focus::Active, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [body, bar] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(ARMED_BAR_HEIGHT)])
        .areas(inner);

    let mut lines = Vec::new();
    let (milestones, issues) = screen
        .results
        .plan
        .as_ref()
        .map(|p| {
            (
                p.milestones.len(),
                p.milestones.iter().map(|m| m.issues.len()).sum::<usize>(),
            )
        })
        .unwrap_or((0, 0));
    lines.push(Line::from(Span::styled(
        format!(
            "Plan · {}s: {milestones} · {}: {issues}",
            screen.milestone_label(),
            screen.issue_label_plural()
        ),
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )));
    for m in screen.results.plan.iter().flat_map(|p| &p.milestones) {
        lines.push(Line::from(Span::styled(
            format!("  {}", sanitize_for_terminal(&m.title)),
            Style::default().fg(theme.accent_info),
        )));
        for i in &m.issues {
            let mut text = format!("    • {}", sanitize_for_terminal(&i.title));
            if !i.labels.is_empty() {
                text.push_str(&format!(
                    "  [{}]",
                    sanitize_for_terminal(&i.labels.join(", "))
                ));
            }
            if !i.blocked_by_titles.is_empty() {
                let deps = i.blocked_by_titles.join(", ");
                text.push_str(&format!("  ← {}", sanitize_for_terminal(&deps)));
            }
            lines.push(Line::from(Span::styled(
                text,
                Style::default().fg(theme.text_secondary),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Will write",
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD),
    )));
    let remote = provider_display_name(screen.provider_kind);
    let will: Vec<_> = run_plan(&screen.config, remote)
        .into_iter()
        .filter(|(h, _)| matches!(*h, "WRITE LOCALLY" | "CREATE ON REMOTE"))
        .collect();
    lines.extend(plan_lines(&will, theme));
    lines.push(Line::from(Span::styled(
        "  not atomic: a failed create stops the run; the result lists what was created",
        Style::default().fg(theme.text_secondary),
    )));
    f.render_widget(
        Paragraph::new(lines).scroll((screen.scroll_offset, 0)),
        body,
    );

    let what = format!(
        "writes .claude/ files, then {milestones} {}(s) and {issues} {} on {remote}",
        screen.milestone_label_lowercase(),
        screen.issue_label_plural_lowercase()
    );
    ArmedBar::new(&screen.review, &what, Instant::now()).render(f, bar, theme);
}
