//! Rendering for [`LaunchDialog`]: what · instructions · mode · spend ·
//! where · warnings · button row (§02.8 `launchDialogKit`, section order and
//! wording kept).

use super::sections::{
    button_line, choice, header, muted, radio, spend_lines, truncate, warn, where_line,
};
use super::{LaunchDialog, LaunchFocus, LaunchMode};
use crate::budget::PreSpawnDecision;
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Most issues listed by name in a multi-issue dialog.
const LIST_MAX: usize = 4;

enum Row {
    Text(Line<'static>),
    Gap,
    /// The instructions box (label row + bordered editor).
    Editor,
}

/// Draw the dialog centered in `area`.
pub fn draw(f: &mut Frame, area: Rect, dialog: &LaunchDialog, theme: &Theme) {
    let width = 86.min(area.width.saturating_sub(4)).max(20.min(area.width));
    let inner_w = width.saturating_sub(4) as usize;
    let rows = rows(dialog, theme, inner_w);
    let gaps = rows.iter().filter(|r| matches!(r, Row::Gap)).count() as u16;
    let lines = rows.iter().filter(|r| matches!(r, Row::Text(_))).count() as u16;
    // Text rows + editor (label + 3-row box) + gaps + button row + border.
    let full = lines + 4 + gaps + 2 + 2;
    let keep_gaps = full <= area.height;
    let height = if keep_gaps { full } else { full - gaps }.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let n = dialog.selected_issues.len();
    let title = if dialog.sessions() > 1 {
        format!("Launch {n} agents")
    } else if dialog.is_multi() {
        format!("Launch 1 agent on {n} issues")
    } else {
        format!("Launch agent on #{}", dialog.selected_issues[0].0)
    };
    let block =
        panel(&title, Focus::Overlay, theme).style(Style::default().bg(theme.surface_overlay));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let body = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    if body.height == 0 {
        return;
    }
    let bottom = body.y + body.height - 1;
    let mut y = body.y;
    for row in rows {
        if y >= bottom {
            break;
        }
        match row {
            Row::Gap if !keep_gaps => {}
            Row::Gap => y += 1,
            Row::Text(line) => {
                f.render_widget(
                    Paragraph::new(line),
                    Rect {
                        y,
                        height: 1,
                        ..body
                    },
                );
                y += 1;
            }
            Row::Editor => {
                let label = if dialog.is_multi() {
                    "Shared instructions (optional)"
                } else {
                    "Instructions (optional)"
                };
                let focused = dialog.focus == LaunchFocus::Instructions;
                let label_style = if focused {
                    theme.accent_info
                } else {
                    theme.text_secondary
                };
                let label = Line::from(Span::styled(label, Style::default().fg(label_style)));
                f.render_widget(
                    Paragraph::new(label),
                    Rect {
                        y,
                        height: 1,
                        ..body
                    },
                );
                let h = 3.min(bottom.saturating_sub(y + 1));
                let boxed = Rect {
                    y: y + 1,
                    height: h,
                    ..body
                };
                let edit_block = theme.styled_block_plain(focused);
                let text_area = edit_block.inner(boxed);
                f.render_widget(edit_block, boxed);
                crate::tui::widgets::input::render(&dialog.editor, f, text_area, theme, focused);
                y += 1 + h;
            }
        }
    }
    let button = Rect {
        y: bottom,
        height: 1,
        ..body
    };
    f.render_widget(Paragraph::new(button_line(dialog, theme)), button);
}

fn rows(d: &LaunchDialog, theme: &Theme, width: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    what_rows(d, theme, width, &mut rows);
    rows.push(Row::Gap);
    if d.is_multi() {
        rows.push(Row::Text(header("SESSIONS", "Ctrl+U", "switch", theme)));
        let n = d.selected_issues.len();
        let (per, uni) = (!d.unified_pr, d.unified_pr);
        rows.push(Row::Text(Line::from(vec![
            radio(per, theme),
            choice(
                &format!("one per issue ({n} worktrees, {n} PRs)   "),
                per,
                theme,
            ),
            radio(uni, theme),
            choice("one unified PR (1 worktree)", uni, theme),
        ])));
        rows.push(Row::Gap);
    }
    rows.push(Row::Editor);
    rows.push(Row::Gap);
    let (key, hint) = match d.focus {
        LaunchFocus::Mode => ("↑↓", "choose"),
        LaunchFocus::Instructions => ("Tab", "to change"),
    };
    rows.push(Row::Text(header("MODE", key, hint, theme)));
    for mode in LaunchMode::ALL {
        rows.push(Row::Text(mode_line(d, mode, theme, width)));
    }
    rows.push(Row::Gap);
    rows.extend(spend_lines(d, theme).into_iter().map(Row::Text));
    rows.push(Row::Gap);
    rows.push(Row::Text(header("WHERE", "", "", theme)));
    rows.push(Row::Text(where_line(d, theme)));
    if let PreSpawnDecision::Warn {
        projected_total,
        limit,
    } = d.budget.decision(d.sessions())
    {
        rows.push(Row::Gap);
        rows.push(Row::Text(Line::from(vec![
            Span::styled("! ", warn(theme).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "projected {} crosses the {}% alert of the {} cap",
                    money(Some(projected_total)),
                    d.budget.alert_threshold_pct,
                    money(Some(limit)),
                ),
                warn(theme),
            ),
        ])));
    }
    rows
}

fn what_rows(d: &LaunchDialog, theme: &Theme, width: usize, rows: &mut Vec<Row>) {
    let id = Style::default().fg(theme.accent_identifier);
    let text = Style::default().fg(theme.text_primary);
    if !d.is_multi() {
        let (n, title) = &d.selected_issues[0];
        let title = truncate(&sanitize_for_terminal(title), width.saturating_sub(8));
        rows.push(Row::Text(Line::from(vec![
            Span::styled(format!("#{n} "), id.add_modifier(Modifier::BOLD)),
            Span::styled(title, text.add_modifier(Modifier::BOLD)),
        ])));
        return;
    }
    for (n, title) in d.selected_issues.iter().take(LIST_MAX) {
        let title = truncate(&sanitize_for_terminal(title), width.saturating_sub(10));
        rows.push(Row::Text(Line::from(vec![
            Span::styled(format!("  {:<7}", format!("#{n}")), id),
            Span::styled(title, text),
        ])));
    }
    let more = d.selected_issues.len().saturating_sub(LIST_MAX);
    if more > 0 {
        rows.push(Row::Text(Line::from(Span::styled(
            format!("  + {more} more"),
            muted(theme),
        ))));
    }
}

fn mode_line(d: &LaunchDialog, mode: LaunchMode, theme: &Theme, width: usize) -> Line<'static> {
    let on = d.mode == mode;
    let lit = on && d.focus == LaunchFocus::Mode;
    let mut label = Style::default().fg(theme.text_primary);
    if on {
        label = label.add_modifier(Modifier::BOLD);
    }
    let mut spans = vec![
        Span::styled(
            if lit { "▌" } else { " " },
            Style::default().fg(theme.border_active),
        ),
        radio(on, theme),
        Span::styled(format!("{:<18}", mode.label()), label),
    ];
    // The consequence is shown whole or not at all: 21 = bar + radio + label.
    if width >= 21 + mode.consequence().chars().count() {
        spans.push(Span::styled(mode.consequence(), muted(theme)));
    }
    let line = Line::from(spans);
    if lit {
        line.style(Style::default().bg(theme.selection_bg))
    } else {
        line
    }
}
