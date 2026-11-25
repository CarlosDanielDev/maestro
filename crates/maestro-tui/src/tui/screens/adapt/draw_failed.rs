//! The Failed step (#1281): the error, and what a failed write had
//! already written (Adapt writes are not atomic).

use super::AdaptScreen;
use super::types::AdaptStep;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn draw_failed(screen: &AdaptScreen, f: &mut Frame, area: Rect, theme: &Theme) {
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
        if error.phase == AdaptStep::Materializing {
            written_lines(screen, theme, &mut lines);
        }
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((screen.scroll_offset, 0)),
        inner,
    );
}

/// #1281: what a failed write had already done, item by item. Adapt
/// writes are not atomic, so the Failed screen never hides them.
fn written_lines(screen: &AdaptScreen, theme: &Theme, lines: &mut Vec<Line<'static>>) {
    let dim = Style::default().fg(theme.text_secondary);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Written before the failure:",
        Style::default()
            .fg(theme.accent_warning)
            .add_modifier(Modifier::BOLD),
    )));
    let created: Vec<_> = screen
        .results
        .scaffold
        .iter()
        .flat_map(|s| &s.files)
        .filter(|f| f.status == crate::adapt::types::ScaffoldFileStatus::Created)
        .collect();
    for file in &created {
        lines.push(Line::from(Span::styled(
            format!("  + {}", sanitize_for_terminal(&file.path)),
            dim,
        )));
    }
    let mat = screen.results.materialize.as_ref();
    for m in mat.iter().flat_map(|m| &m.milestones_created) {
        let how = if m.reused { "reused" } else { "created" };
        lines.push(Line::from(Span::styled(
            format!(
                "  {} #{} {} ({how})",
                screen.milestone_label_lowercase(),
                m.number,
                sanitize_for_terminal(&m.title)
            ),
            dim,
        )));
    }
    let issues = mat
        .iter()
        .flat_map(|m| m.issues_created.iter().chain(&m.tech_debt_issue));
    let mut any = !created.is_empty() || mat.is_some_and(|m| !m.milestones_created.is_empty());
    for i in issues {
        any = true;
        lines.push(Line::from(Span::styled(
            format!("  #{} {}", i.number, sanitize_for_terminal(&i.title)),
            dim,
        )));
    }
    if !any {
        lines.push(Line::from(Span::styled("  nothing", dim)));
    }
}
