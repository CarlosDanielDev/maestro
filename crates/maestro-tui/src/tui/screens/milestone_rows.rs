//! Row builders of the Milestones tab (#1269, design §02.9): one row per
//! milestone with its START count, the issue rows grouped by plan level
//! with readiness chips, and the "Run this milestone" card.

use super::milestone::MilestoneEntry;
use super::milestone_plan::{IssueReadiness, MilestonePlan};
use super::sanitize_for_terminal;
use crate::provider::types::Issue;
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::gauge::bar;
use crate::tui::ds::money::money;
use crate::tui::icons::{IconId, get_for_mode};
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// Below this width the progress bar drops; counts and START never do.
pub const BAR_MIN_WIDTH: usize = 40;
const BAR_CELLS: usize = 8;

/// `▌name  ▓▓░░░░░░ 3/11  6 ready`. `ready` is `None` when the plan failed.
pub fn milestone_row(
    e: &MilestoneEntry,
    ready: Option<usize>,
    width: usize,
    selected: bool,
    theme: &Theme,
    nerd: bool,
) -> Line<'static> {
    let base = if selected {
        focused_selection_style(theme)
    } else {
        Style::default().fg(theme.text_primary)
    };
    let cursor = if selected { "▌" } else { " " };
    let left = vec![
        Span::styled(cursor.to_string(), base),
        Span::styled(sanitize_for_terminal(&e.title), base),
    ];
    let mut right = Vec::new();
    if width >= BAR_MIN_WIDTH {
        let (filled, empty) = bar(e.progress_ratio(), BAR_CELLS, nerd);
        right.push(Span::styled(filled, Style::default().fg(theme.accent_info)));
        right.push(Span::styled(
            empty,
            Style::default().fg(theme.gauge_background),
        ));
        right.push(Span::raw(" "));
    }
    right.push(Span::styled(
        format!("{:>5}", format!("{}/{}", e.closed_issues, e.total_issues())),
        Style::default().fg(theme.text_secondary),
    ));
    let (text, style) = match ready {
        // 0 ready is a fact, not a failure: muted, never red.
        Some(0) => ("0 ready".to_string(), Style::default().fg(theme.text_muted)),
        Some(n) => (
            format!("{n} ready"),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ),
        None => (
            "? ready".to_string(),
            Style::default().fg(theme.accent_warning),
        ),
    };
    right.push(Span::styled(format!(" {text:>8}"), style));
    let mut line = row_line(left, right, width);
    if selected {
        for span in &mut line.spans {
            span.style = base;
        }
    }
    line
}

/// The legend under the list: what START means, never left to colour.
pub fn legend(theme: &Theme, width: usize) -> Line<'static> {
    let line = Line::from(vec![
        Span::styled(
            " ready",
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " = wave 1: no open blocker, no claim",
            Style::default().fg(theme.text_muted),
        ),
    ]);
    ellipsize_line(line, width)
}

/// `○ 6 ready · 3 waiting · ⊘ 1 blocked · ◐ 1 claimed`.
pub fn summary(
    plan: &MilestonePlan,
    issues: &[&Issue],
    theme: &Theme,
    nerd: bool,
) -> Line<'static> {
    let mut counts = [0usize; 4];
    for i in issues {
        match plan.readiness(i.number) {
            IssueReadiness::Ready => counts[0] += 1,
            IssueReadiness::Waits(_) => counts[1] += 1,
            IssueReadiness::BlockedOutside { .. } | IssueReadiness::Unknown => counts[2] += 1,
            IssueReadiness::Claimed => counts[3] += 1,
        }
    }
    let parts = [
        (get_for_mode(IconId::ChipQueued, nerd), counts[0], "ready"),
        (get_for_mode(IconId::Hourglass, nerd), counts[1], "waiting"),
        (get_for_mode(IconId::XCircle, nerd), counts[2], "blocked"),
        (
            get_for_mode(IconId::ChipNeedsReview, nerd),
            counts[3],
            "claimed",
        ),
    ];
    let text = parts
        .iter()
        .map(|(icon, n, word)| format!("{icon} {n} {word}"))
        .collect::<Vec<_>>()
        .join(" · ");
    Line::from(Span::styled(
        format!(" {text}"),
        Style::default().fg(theme.text_secondary),
    ))
}

/// One issue row: cursor, chip, number, title, and the blocker on the right.
pub fn issue_row(
    issue: &Issue,
    readiness: IssueReadiness,
    width: usize,
    focused: bool,
    theme: &Theme,
    nerd: bool,
) -> Line<'static> {
    let chip_color = match readiness {
        IssueReadiness::Ready => theme.accent_success,
        IssueReadiness::Waits(_) => theme.text_secondary,
        IssueReadiness::Claimed => theme.accent_info,
        IssueReadiness::BlockedOutside { .. } | IssueReadiness::Unknown => theme.accent_warning,
    };
    let pick = |s: Style| {
        if focused {
            focused_selection_style(theme)
        } else {
            s
        }
    };
    let left = vec![
        Span::styled(
            if focused { "> " } else { "  " }.to_string(),
            pick(Style::default()),
        ),
        Span::styled(
            format!("{:<11}", readiness_chip(readiness, nerd)),
            pick(Style::default().fg(chip_color)),
        ),
        Span::styled(
            format!(" #{} ", issue.number),
            pick(
                Style::default()
                    .fg(theme.accent_identifier)
                    .add_modifier(Modifier::BOLD),
            ),
        ),
        Span::styled(
            sanitize_for_terminal(&issue.title),
            pick(Style::default().fg(theme.text_primary)),
        ),
    ];
    let arrow = get_for_mode(IconId::ArrowLeft, nerd);
    let note = match readiness {
        IssueReadiness::Waits(by) => format!("{arrow} #{by}"),
        IssueReadiness::BlockedOutside {
            by,
            milestone: Some(m),
        } => format!("{arrow} #{by} m{m}"),
        IssueReadiness::BlockedOutside { by, .. } => format!("{arrow} #{by} not fetched"),
        _ => String::new(),
    };
    let right = vec![Span::styled(
        note,
        pick(Style::default().fg(theme.text_muted)),
    )];
    row_line(left, right, width)
}

/// The "Run this milestone" card: what `R` would do, so it is never a
/// surprise. `7 of 11 run in 2 waves · worst case $35.00 · 4 wait`.
pub fn run_card(plan: &MilestonePlan, theme: &Theme) -> Vec<Line<'static>> {
    let runs = plan.runs();
    let waves = plan.plan.waves.len();
    let held = plan.plan.deferred.len();
    let mut text = format!(
        " {runs} of {} run in {waves} wave{}",
        plan.open,
        if waves == 1 { "" } else { "s" }
    );
    if plan.plan.worst_case_usd > 0.0 {
        text.push_str(&format!(
            " · worst case {}",
            money(Some(plan.plan.worst_case_usd))
        ));
    }
    if held > 0 {
        text.push_str(&format!(" · {held} wait"));
    }
    vec![
        Line::from(Span::styled(
            " Run this milestone",
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(text, Style::default().fg(theme.text_primary)),
            Span::styled("  R plan the run", Style::default().fg(theme.accent_info)),
        ]),
    ]
}

/// `left` cut with `…` to leave room for `note`, then `note` right-aligned.
pub fn row_line(
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    let right_w: usize = right.iter().map(|s| s.content.width()).sum();
    let room = width.saturating_sub(right_w + usize::from(right_w > 0));
    let mut line = ellipsize_line(Line::from(left), room);
    let pad = width.saturating_sub(line.width() + right_w);
    line.spans.push(Span::raw(" ".repeat(pad)));
    line.spans.extend(right);
    line
}

/// Readiness chip of one issue: glyph (ASCII fallback) and word.
pub fn readiness_chip(r: IssueReadiness, nerd: bool) -> String {
    let (icon, word) = match r {
        IssueReadiness::Ready => (IconId::ChipQueued, "ready"),
        IssueReadiness::Waits(_) => (IconId::Hourglass, "waits"),
        IssueReadiness::BlockedOutside { .. } => (IconId::XCircle, "blocked"),
        IssueReadiness::Claimed => (IconId::ChipNeedsReview, "claimed"),
        IssueReadiness::Unknown => (IconId::Warning, "no data"),
    };
    format!("{} {word}", get_for_mode(icon, nerd))
}
