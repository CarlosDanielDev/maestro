//! Dashboard layout (§02.1): the view strip, the Work list and the detail
//! of the selected row. Wide and Regular put the detail right of the list;
//! Compact stacks them. The Activity pane under it is the shell's.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::rows::{Group, WorkKey};
use super::{Dashboard, DashboardView, session_cost, session_title};
use crate::provider::github::types::{PendingPr, PendingPrStatus};
use crate::provider::types::{CheckConclusion, CheckRun, CheckStatus};
use crate::session::types::Session;
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::icons::{self, IconId};
use crate::tui::screens::landing::draw::ellipsize;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

/// Work list width on Wide and Regular (the design's 66 and 58 columns).
const LIST_W_WIDE: u16 = 66;
const LIST_W_REGULAR: u16 = 58;
/// Widest chip (`⊠ gates failed`) plus one space.
const STATUS_W: usize = 16;
const ISSUE_W: usize = 6;

/// Draw the Dashboard in `area`; `dispatch::sync` rebuilt the rows.
/// `bp` is the terminal's breakpoint.
pub fn draw(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &mut Dashboard,
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    dash.armed.tick(dash.now());
    let [strip, main] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    draw_view_strip(f, strip, view, bp, theme);
    let (list, detail) = match bp {
        Breakpoint::Wide | Breakpoint::Regular => {
            let w = if bp == Breakpoint::Wide {
                LIST_W_WIDE
            } else {
                LIST_W_REGULAR
            };
            let [l, d] =
                Layout::horizontal([Constraint::Length(w), Constraint::Min(0)]).areas(main);
            (l, d)
        }
        // Compact with nothing to select: the list takes the whole body.
        _ if dash.list.selected.is_none() => (main, Rect::default()),
        _ => {
            let want = dash.list.rows.len() as u16 + 3;
            let h = want.min(main.height / 2).max(3);
            let [l, d] = Layout::vertical([Constraint::Length(h), Constraint::Min(0)]).areas(main);
            (l, d)
        }
    };
    draw_work_list(f, list, view, dash, theme, nerd_font);
    if !detail.is_empty() {
        let tall = bp != Breakpoint::Compact;
        super::detail::draw(f, detail, view, dash, tall, theme, nerd_font);
    }
}

/// `o Ops · c Cost · t Tokens · g Deps`; Ops is the only view drawn
/// here (#1280), `c` and `t` open the Cost and Token screens, `g` is not
/// bound yet so it is muted. Right side: the one screen summary.
fn draw_view_strip(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    bp: Breakpoint,
    theme: &Theme,
) {
    let sel = focused_selection_style(theme);
    let muted = Style::new().fg(theme.text_muted);
    let mut spans = vec![Span::raw(" "), Span::styled(" o Ops ", sel)];
    for (k, label, bound) in [
        ("c", "Cost", true),
        ("t", "Tokens", true),
        ("g", "Deps", false),
    ] {
        spans.push(Span::raw("  "));
        let key = if bound { key_style(theme) } else { muted };
        spans.push(Span::styled(k, key));
        spans.push(Span::styled(format!(" {label}"), muted));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    if bp == Breakpoint::Compact {
        return;
    }
    let live = view
        .sessions
        .iter()
        .filter(|s| !s.status.is_terminal())
        .count();
    let pipe = view.ci.len() + view.pending.len();
    let summary = format!("{live} live · {pipe} in PR pipeline ");
    let w = summary.width() as u16;
    if w < area.width / 2 {
        let r = Rect::new(area.x + area.width - w, area.y, w, 1);
        f.render_widget(Paragraph::new(Span::styled(summary, muted)), r);
    }
}

fn draw_work_list(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    theme: &Theme,
    nerd_font: bool,
) {
    let count = dash.list.rows.iter().filter(|k| !k.is_header()).count();
    let mut block = panel(&format!("Work ({count})"), Focus::Active, theme);
    let inner = block.inner(area);
    let width = inner.width as usize;
    let (lines, selected_line) = list_lines(view, dash, width, theme, nerd_font);
    let height = inner.height as usize;
    let offset = selected_line.map_or(0, |at| (at + 1).saturating_sub(height));
    // Rows below the fold; headers, blanks and empty-group lines do not count.
    let hidden = lines
        .iter()
        .skip(offset + height)
        .filter(|(_, is_row)| *is_row)
        .count();
    if hidden > 0 {
        let more = Span::styled(format!(" ↓ {hidden} more "), key_style(theme));
        block = block.title_bottom(Line::from(more).right_aligned());
    }
    f.render_widget(block, area);
    let shown: Vec<Line> = lines
        .into_iter()
        .skip(offset)
        .take(height)
        .map(|(l, _)| l)
        .collect();
    f.render_widget(Paragraph::new(shown), inner);
}

/// The list as lines (each with "is a row"), and the line of the selected row.
fn list_lines(
    view: &DashboardView<'_>,
    dash: &Dashboard,
    width: usize,
    theme: &Theme,
    nerd_font: bool,
) -> (Vec<(Line<'static>, bool)>, Option<usize>) {
    let mut lines = Vec::new();
    let mut selected_line = None;
    for key in &dash.list.rows {
        let line = match *key {
            WorkKey::Header(group) => {
                let rows = dash.list.group_rows(group).len();
                if group == Group::Finished && rows == 0 {
                    continue;
                }
                if !lines.is_empty() {
                    lines.push((Line::raw(""), false));
                }
                lines.push((header_line(group, rows, theme), false));
                if rows == 0 {
                    lines.push((empty_line(group, theme), false));
                }
                continue;
            }
            WorkKey::Session(id) => match view.session(id) {
                Some(s) => session_line(s, view, width, theme, nerd_font),
                None => continue,
            },
            WorkKey::PendingPr(issue) => match view.pending_pr(issue) {
                Some(p) => pending_line(p, width, theme, nerd_font),
                None => continue,
            },
            WorkKey::OpenPr(pr) => {
                let checks = view.ci.get(&pr).map_or(&[][..], Vec::as_slice);
                open_pr_line(pr, checks, width, theme, nerd_font)
            }
        };
        // One mark only, even when two rows share a key (test fixtures).
        let selected = selected_line.is_none() && dash.list.selected == Some(*key);
        if selected {
            selected_line = Some(lines.len());
        }
        lines.push((mark(line, selected, width, theme), true));
    }
    (lines, selected_line)
}

fn header_line(group: Group, rows: usize, theme: &Theme) -> Line<'static> {
    let label = match group {
        Group::Sessions => "SESSIONS",
        Group::PrPipeline => "PR PIPELINE",
        Group::Finished => "FINISHED · this run",
    };
    let bold = Style::new().add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(format!(" {label}"), bold.fg(theme.text_muted)),
        Span::styled(format!("  {rows}"), bold.fg(theme.text_primary)),
    ])
}

fn empty_line(group: Group, theme: &Theme) -> Line<'static> {
    let muted = Style::new().fg(theme.text_muted);
    match group {
        Group::PrPipeline => Line::from(Span::styled(
            "   ○ No PRs waiting. Finished work shows up here.",
            muted,
        )),
        _ => Line::from(vec![
            Span::styled("   ○ Nothing running · start work from ", muted),
            Span::styled("1", key_style(theme)),
            Span::styled(" Home", muted),
        ]),
    }
}

/// `▌` and the focused selection style on the selected row, cut with `…`.
fn mark(line: Line<'static>, selected: bool, width: usize, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::raw(if selected { "▌" } else { " " })];
    spans.extend(line.spans);
    let line = ellipsize_line(Line::from(spans), width);
    if selected {
        line.style(focused_selection_style(theme))
    } else {
        line
    }
}

/// `chip  #412  title  42%  $1.20  12m03s`; ctx from 52 columns, time from 60.
fn session_line(
    s: &Session,
    view: &DashboardView<'_>,
    width: usize,
    theme: &Theme,
    nerd_font: bool,
) -> Line<'static> {
    let chip = status_chip_for_mode(s.status, theme, nerd_font);
    let mut tail = String::new();
    if width >= 52 {
        tail.push_str(&format!(
            "{:>5}",
            format!("{:.0}%", s.context_pct.clamp(0.0, 1.0) * 100.0)
        ));
    }
    let cost = session_cost(s);
    tail.push_str(&format!("{:>8}", money(cost)));
    if width >= 60 {
        tail.push_str(&format!(
            "{:>8}",
            crate::tui::app::mission::elapsed(s, view.now)
        ));
    }
    let title = session_title(s);
    row_line(chip, s.issue_number, &title, tail, width, theme)
}

fn row_line(
    chip: Span<'static>,
    issue: Option<u64>,
    title: &str,
    tail: String,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let gap = STATUS_W.saturating_sub(chip.content.width()).max(1);
    let issue = issue.map(|n| format!("#{n}")).unwrap_or_default();
    let title_w = width.saturating_sub(1 + STATUS_W + ISSUE_W + tail.width() + 1);
    let title = ellipsize(&sanitize_for_terminal(title), title_w);
    Line::from(vec![
        Span::styled(format!("{}{}", chip.content, " ".repeat(gap)), chip.style),
        Span::styled(
            format!("{issue:<ISSUE_W$}"),
            Style::new()
                .fg(theme.accent_identifier)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{title:<title_w$}"),
            Style::new().fg(theme.text_primary),
        ),
        Span::styled(tail, Style::new().fg(theme.text_muted)),
    ])
}

/// The chip of a pending PR: `◐ needs retry`, `○ auto-retry`, `↻ retrying`,
/// `✗ stopped` (the four `PendingPrStatus` glyphs of §02.1).
pub fn pending_chip(status: PendingPrStatus, theme: &Theme, nerd_font: bool) -> Span<'static> {
    let (icon, word, style) = match status {
        PendingPrStatus::AwaitingManualRetry => (
            IconId::ChipNeedsReview,
            "needs retry",
            Style::new()
                .fg(theme.status_needs_pr)
                .add_modifier(Modifier::BOLD),
        ),
        PendingPrStatus::RetryScheduled => (
            IconId::ChipQueued,
            "auto-retry",
            Style::new().fg(theme.text_muted),
        ),
        PendingPrStatus::Retrying => (
            IconId::ChipRetrying,
            "retrying",
            Style::new().fg(theme.status_retrying),
        ),
        PendingPrStatus::PermanentlyFailed => (
            IconId::ChipErrored,
            "stopped",
            Style::new()
                .fg(theme.accent_error)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let glyph = icons::get_for_mode(icon, nerd_font);
    Span::styled(format!("{glyph} {word}"), style)
}

fn pending_line(p: &PendingPr, width: usize, theme: &Theme, nerd_font: bool) -> Line<'static> {
    let chip = pending_chip(p.status, theme, nerd_font);
    let tail = format!(" manual {}/{}", p.manual_retry_count, MANUAL_CAP);
    row_line(chip, Some(p.issue_number), &p.branch, tail, width, theme)
}

pub(super) const MANUAL_CAP: u32 =
    crate::provider::github::types::PENDING_PR_MANUAL_RETRY_LIFETIME_CAP;

/// `◆ CI running`, `↯ CI failing` or `✓ CI passed`, and `checks n/m`.
pub fn ci_chip(checks: &[CheckRun], theme: &Theme, nerd_font: bool) -> Span<'static> {
    let failed = checks.iter().any(|c| {
        matches!(
            c.conclusion,
            CheckConclusion::Failure | CheckConclusion::Cancelled
        )
    });
    let done = !checks.is_empty() && checks.iter().all(|c| c.status == CheckStatus::Completed);
    let (icon, word, color) = if failed {
        (IconId::ChipCiFix, "CI failing", theme.accent_error)
    } else if done {
        (IconId::ChipCompleted, "CI passed", theme.accent_success)
    } else {
        (IconId::ChipGatesRunning, "CI running", theme.accent_info)
    };
    let glyph = icons::get_for_mode(icon, nerd_font);
    Span::styled(format!("{glyph} {word}"), Style::new().fg(color))
}

fn open_pr_line(
    pr: u64,
    checks: &[CheckRun],
    width: usize,
    theme: &Theme,
    nerd_font: bool,
) -> Line<'static> {
    let done = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Completed)
        .count();
    let tail = format!(" checks {done}/{}", checks.len());
    let chip = ci_chip(checks, theme, nerd_font);
    row_line(chip, None, &format!("PR #{pr}"), tail, width, theme)
}
