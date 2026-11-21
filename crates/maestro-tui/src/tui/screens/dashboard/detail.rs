//! The detail pane of the selected Work row (§02.1). Session: status,
//! model, elapsed, cost, retries, worktree, the `run ─ gates ─ push ─ PR ─
//! CI` stepper, gates, context · cost and the output tail. Pending PR:
//! attempts, the kept errors and the exact write behind the armed bar.
//! Open PR: its CI checks.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::draw::{MANUAL_CAP, pending_chip};
use super::rows::{WorkKey, pipeline_stage};
use super::{
    Dashboard, DashboardView, pr_lock_reason, pr_write_description, session_cost, session_title,
};
use crate::provider::github::types::PendingPr;
use crate::session::types::Session;
use crate::tui::ds::armed::ArmedEnter;
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::gauge::bar;
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::ds::stepper::{Steps, stepper_line};
use crate::tui::icons::{self, IconId};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::CiMonitorWidget;
use crate::tui::widgets::armed_bar::{ARMED_BAR_HEIGHT, ArmedBar};
use crate::util::formatting::format_tokens;

const STEPS: [&str; 5] = ["run", "gates", "push", "PR", "CI"];
const CONTEXT_BAR_W: usize = 12;

pub fn draw(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    tall: bool,
    theme: &Theme,
    nerd_font: bool,
) {
    match dash.list.selected {
        Some(WorkKey::Session(id)) => match view.session(id) {
            Some(s) => session(f, area, s, view, tall, theme, nerd_font),
            None => empty(f, area, theme),
        },
        Some(WorkKey::PendingPr(issue)) => match view.pending_pr(issue) {
            Some(p) => pending(f, area, p, view, dash, tall, theme, nerd_font),
            None => empty(f, area, theme),
        },
        Some(WorkKey::OpenPr(pr)) => {
            let checks = view.ci.get(&pr).map_or(&[][..], Vec::as_slice);
            CiMonitorWidget::new(checks, theme)
                .pr_number(pr)
                .render(area, f.buffer_mut());
        }
        _ => empty(f, area, theme),
    }
}

fn empty(f: &mut Frame, area: Rect, theme: &Theme) {
    let block = panel("Detail", Focus::Inactive, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let text = " Nothing selected. A session or PR row shows its gates, output and PR stage here.";
    f.render_widget(
        Paragraph::new(Span::styled(text, Style::new().fg(theme.text_muted)))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn muted(theme: &Theme) -> Style {
    Style::new().fg(theme.text_muted)
}

fn section(label: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {label}"),
        muted(theme).add_modifier(Modifier::BOLD),
    ))
}

/// ` PIPELINE  ✓ run ─ ✗ gates ─ ○ push ─ ○ PR ─ ○ CI` through
/// `ds::stepper` (#1274): a failed step is `✗` + error colour, never colour
/// alone; a narrow terminal gets the `Step n/N` form.
fn stepper(
    stage: usize,
    failed: Option<usize>,
    frame_width: u16,
    width: u16,
    theme: &Theme,
) -> Line<'static> {
    let label = " PIPELINE ";
    let errors: Vec<usize> = failed.into_iter().collect();
    let steps = Steps {
        labels: &STEPS,
        current: stage,
        errors: &errors,
    };
    let room = width.saturating_sub(label.len() as u16);
    let mut spans = vec![Span::styled(
        label,
        muted(theme).add_modifier(Modifier::BOLD),
    )];
    spans.extend(stepper_line(steps, frame_width, room, theme).spans);
    Line::from(spans)
}

fn render_lines(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>, theme: &Theme) {
    let block = panel(title, Focus::Inactive, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let lines: Vec<Line> = lines
        .into_iter()
        .map(|l| ellipsize_line(l, width))
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn session(
    f: &mut Frame,
    area: Rect,
    s: &Session,
    view: &DashboardView<'_>,
    tall: bool,
    theme: &Theme,
    nerd_font: bool,
) {
    let text = Style::new().fg(theme.text_primary);
    let title = sanitize_for_terminal(&session_title(s));
    let head = match s.issue_number {
        Some(n) => format!("#{n} {title}"),
        None => title.clone(),
    };
    let cost = session_cost(s);
    let dot = || Span::styled(" · ", muted(theme));
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        status_chip_for_mode(s.status, theme, nerd_font),
        Span::raw("   "),
        Span::styled(
            sanitize_for_terminal(&s.model),
            Style::new().fg(theme.accent_info),
        ),
        dot(),
        Span::styled(crate::tui::app::mission::elapsed(s, view.now), text),
        dot(),
        Span::styled(money(cost), text),
        Span::styled(" · retry ", muted(theme)),
        Span::styled(s.retry_count.to_string(), text),
    ])];
    lines.push(Line::from(Span::styled(
        format!(" {title}"),
        text.add_modifier(Modifier::BOLD),
    )));
    if tall {
        let wt = s
            .worktree_path
            .as_ref()
            .map_or("no worktree".to_string(), |p| {
                sanitize_for_terminal(&p.display().to_string())
            });
        lines.push(Line::from(Span::styled(format!(" {wt}"), muted(theme))));
        lines.push(Line::raw(""));
    }
    let (stage, failed) = pipeline_stage(s.status);
    lines.push(stepper(
        stage,
        failed,
        f.area().width,
        area.width.saturating_sub(2),
        theme,
    ));
    if tall {
        lines.push(Line::raw(""));
    }
    lines.push(section("GATES", theme));
    if s.gate_results.is_empty() {
        lines.push(Line::from(Span::styled(
            " ○ no gate results yet",
            muted(theme),
        )));
    }
    for g in &s.gate_results {
        let (icon, style) = if g.passed {
            (IconId::ChipCompleted, Style::new().fg(theme.accent_success))
        } else {
            (
                IconId::ChipErrored,
                Style::new()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let glyph = icons::get_for_mode(icon, nerd_font);
        lines.push(Line::from(vec![
            Span::styled(format!(" {glyph} "), style),
            Span::styled(format!("{:<9}", sanitize_for_terminal(&g.gate)), text),
            Span::styled(sanitize_for_terminal(&g.message), muted(theme)),
        ]));
    }
    lines.push(section("CONTEXT · COST", theme));
    let (filled, rest) = bar(s.context_pct, CONTEXT_BAR_W, nerd_font);
    let level = if s.context_pct >= 0.8 {
        theme.accent_warning
    } else {
        theme.accent_info
    };
    lines.push(Line::from(vec![
        Span::styled(" context ", muted(theme)),
        Span::styled(filled, Style::new().fg(level)),
        Span::styled(rest, muted(theme)),
        Span::styled(
            format!(" {:.0}%", s.context_pct.clamp(0.0, 1.0) * 100.0),
            text,
        ),
    ]));
    let t = &s.token_usage;
    lines.push(Line::from(vec![
        Span::styled(" tokens  ", muted(theme)),
        Span::styled(
            format!(
                "{} in · {} out",
                format_tokens(t.input_tokens),
                format_tokens(t.output_tokens)
            ),
            text,
        ),
        Span::styled(format!(" · files {}", s.files_touched.len()), muted(theme)),
    ]));
    lines.push(section("OUTPUT · last lines", theme));
    let room = (area.height as usize).saturating_sub(lines.len() + 2);
    if s.activity_log.is_empty() {
        lines.push(Line::from(Span::styled(" no output yet", muted(theme))));
    }
    let skip = s.activity_log.len().saturating_sub(room);
    for e in s.activity_log.iter().skip(skip) {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", e.timestamp.format("%H:%M:%S")),
                muted(theme),
            ),
            Span::styled(sanitize_for_terminal(&e.message), text),
        ]));
    }
    render_lines(f, area, &head, lines, theme);
}

#[allow(clippy::too_many_arguments)]
fn pending(
    f: &mut Frame,
    area: Rect,
    p: &PendingPr,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    tall: bool,
    theme: &Theme,
    nerd_font: bool,
) {
    let text = Style::new().fg(theme.text_primary);
    let id = Style::new().fg(theme.accent_identifier);
    let mut lines = vec![
        Line::from(vec![
            Span::raw(" "),
            pending_chip(p.status, theme, nerd_font),
            Span::styled("   restored from last run · ", muted(theme)),
            Span::styled(
                format!(
                    "attempt {}/{} · manual {}/{MANUAL_CAP}",
                    p.attempt, p.max_attempts, p.manual_retry_count
                ),
                text,
            ),
        ]),
        Line::from(vec![
            Span::styled(format!(" {}", sanitize_for_terminal(&p.branch)), id),
            Span::styled(" → ", muted(theme)),
            Span::styled(sanitize_for_terminal(&p.base_branch), id),
            Span::styled(
                format!(
                    "   {} files · {} spent",
                    p.files_touched.len(),
                    money(Some(p.cost_usd))
                ),
                muted(theme),
            ),
        ]),
    ];
    if tall {
        lines.push(Line::raw(""));
    }
    lines.push(stepper(
        3,
        Some(3),
        f.area().width,
        area.width.saturating_sub(2),
        theme,
    ));
    if tall {
        lines.push(Line::raw(""));
    }
    lines.push(section(
        &format!(
            "LAST ERRORS  {}/{} kept",
            p.last_errors.len(),
            crate::provider::github::types::PENDING_PR_LAST_ERRORS_CAP
        ),
        theme,
    ));
    if p.last_errors.is_empty() {
        lines.push(Line::from(Span::styled(" no error kept", muted(theme))));
    }
    let err = icons::get_for_mode(IconId::ChipErrored, nerd_font);
    for e in &p.last_errors {
        lines.push(Line::from(vec![
            Span::styled(format!(" {err} "), Style::new().fg(theme.accent_error)),
            Span::styled(sanitize_for_terminal(e), text),
        ]));
    }
    if !view.gh_auth_ok {
        lines.push(Line::from(Span::styled(
            " gh auth expired · run gh auth login in a shell, then ⏎ ⏎ retries",
            Style::new().fg(theme.accent_warning),
        )));
    }
    let title = format!("#{} pending PR", p.issue_number);
    let bar_h = ARMED_BAR_HEIGHT.min(area.height.saturating_sub(2));
    let [body, bar_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(bar_h)]).areas(area);
    render_lines(f, body, &title, lines, theme);
    let armed = match pr_lock_reason(p) {
        Some(reason) => {
            let mut blocked = ArmedEnter::new();
            blocked.block(reason);
            blocked
        }
        None if dash.armed_for == Some(p.issue_number) => dash.armed.clone(),
        None => ArmedEnter::new(),
    };
    let write = pr_write_description(p);
    ArmedBar::new(&armed, &write, dash.now()).render(f, bar_area, theme);
}
