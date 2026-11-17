//! Mission Control layout at the three breakpoints (design §02 `home()`):
//!
//! - Wide: Actions + Milestone · Needs you / Fleet · Budget & quota (the
//!   rest of the right column is the Insight slot, filled by a later issue).
//! - Regular: the right column drops; quota is a one-line strip on top.
//! - Compact: one column, strip / Needs you / Fleet; Actions run by key.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{
    HomeFocus, MilestoneCard, MissionControl, MissionView, QuotaRow, actions, disabled_reason,
};
use crate::tui::ds::attention::{self, QueueView};
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::fleet_table;
use crate::tui::ds::gauge::{bar, gauge_level};
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::screens::landing::draw::ellipsize;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

const WIDE_LEFT: u16 = 26;
const WIDE_RIGHT: u16 = 44;
const REGULAR_LEFT: u16 = 24;
/// Border + one strip row + border.
const STRIP_H: u16 = 3;
/// Header + two rows + borders: the Fleet table never shrinks below this.
const FLEET_MIN_H: u16 = 5;
const DISABLED: &str = "⊘";

pub fn draw(
    f: &mut Frame,
    area: Rect,
    view: &MissionView,
    state: &mut MissionControl,
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    state.actions_visible = !matches!(bp, Breakpoint::Compact | Breakpoint::TooSmall);
    if !state.actions_visible {
        state.focus = HomeFocus::NeedsYou;
    }
    state.attention_selected = state
        .attention_selected
        .min(view.attention.len().saturating_sub(1));
    let attention_h = view.attention.len().max(3) as u16 + 2;
    let ctx = Ctx {
        view,
        state,
        theme,
        nerd_font,
    };

    match bp {
        Breakpoint::Wide => {
            let [left, center, right] = Layout::horizontal([
                Constraint::Length(WIDE_LEFT),
                Constraint::Min(0),
                Constraint::Length(WIDE_RIGHT),
            ])
            .areas(area);
            ctx.left_column(f, left);
            let [needs, fleet] = Layout::vertical([
                Constraint::Length(attention_h),
                Constraint::Min(FLEET_MIN_H),
            ])
            .areas(center);
            ctx.needs_you(f, needs);
            ctx.fleet(f, fleet, false);
            let budget_h = view.budget.len() as u16 + 2;
            let [budget, _insight_slot] =
                Layout::vertical([Constraint::Length(budget_h), Constraint::Min(0)]).areas(right);
            ctx.budget_panel(f, budget);
        }
        Breakpoint::Regular => {
            let [left, center] =
                Layout::horizontal([Constraint::Length(REGULAR_LEFT), Constraint::Min(0)])
                    .areas(area);
            ctx.left_column(f, left);
            ctx.strip_needs_fleet(f, center, attention_h, false);
        }
        Breakpoint::Compact | Breakpoint::TooSmall => {
            ctx.strip_needs_fleet(f, area, attention_h, true);
        }
    }
}

struct Ctx<'a> {
    view: &'a MissionView,
    state: &'a MissionControl,
    theme: &'a Theme,
    nerd_font: bool,
}

impl Ctx<'_> {
    fn left_column(&self, f: &mut Frame, area: Rect) {
        let lines = self.action_lines(area.width.saturating_sub(2) as usize);
        let [acts, milestone] = Layout::vertical([
            Constraint::Length(lines.len() as u16 + 2),
            Constraint::Min(0),
        ])
        .areas(area);
        let block = self
            .theme
            .styled_block("Actions", self.state.focus == HomeFocus::Actions);
        let inner = block.inner(acts);
        f.render_widget(block, acts);
        f.render_widget(Paragraph::new(lines), inner);
        self.milestone(f, milestone);
    }

    fn strip_needs_fleet(&self, f: &mut Frame, area: Rect, attention_h: u16, compact: bool) {
        let [strip, needs, fleet] = Layout::vertical([
            Constraint::Length(STRIP_H),
            Constraint::Length(attention_h),
            Constraint::Min(FLEET_MIN_H),
        ])
        .areas(area);
        self.quota_strip(f, strip);
        self.needs_you(f, needs);
        self.fleet(f, fleet, compact);
    }

    fn needs_you(&self, f: &mut Frame, area: Rect) {
        let view = QueueView {
            focused: self.state.focus == HomeFocus::NeedsYou,
            selected: self.state.attention_selected,
            fleet_busy: self.view.fleet_busy,
            now: self.view.now,
        };
        attention::draw(
            f,
            area,
            &self.view.attention,
            view,
            self.theme,
            self.nerd_font,
        );
    }

    fn fleet(&self, f: &mut Frame, area: Rect, compact: bool) {
        fleet_table::draw(
            f,
            area,
            &self.view.fleet,
            compact,
            self.theme,
            self.nerd_font,
        );
    }

    /// RUN · PLAN · INSIGHT, then one `⊘ key reason` line per disabled action.
    fn action_lines(&self, width: usize) -> Vec<Line<'static>> {
        let t = self.theme;
        let header = Style::new().fg(t.text_muted).add_modifier(Modifier::BOLD);
        let off = Style::new().fg(t.text_muted).add_modifier(Modifier::DIM);
        let focused = self.state.focus == HomeFocus::Actions;
        let all = actions();
        let mut lines = Vec::new();
        for (group, keys) in super::ACTION_GROUPS {
            lines.push(Line::from(Span::styled(format!(" {group}"), header)));
            for key in keys.iter() {
                let Some((idx, a)) = all.iter().enumerate().find(|(_, a)| a.key == *key) else {
                    continue;
                };
                let selected = focused && idx == self.state.action_selected;
                let reason = disabled_reason(a.key, &self.view.gate);
                let (key_s, label_s) = match reason {
                    Some(_) => (off, off),
                    None => (key_style(t), Style::new().fg(t.text_primary)),
                };
                // Marker, key, space, label, space, `⊘`, space.
                let label_w = width.saturating_sub(6);
                let label = ellipsize(a.label, label_w);
                let mut spans = vec![
                    Span::raw(if selected { "▌" } else { " " }),
                    Span::styled(a.key.to_string(), key_s),
                    Span::raw(" "),
                    Span::styled(format!("{label:<label_w$}"), label_s),
                    Span::styled(
                        if reason.is_some() {
                            format!(" {DISABLED}")
                        } else {
                            "  ".to_string()
                        },
                        Style::new().fg(t.text_muted),
                    ),
                ];
                if selected {
                    let sel = focused_selection_style(t);
                    for span in &mut spans {
                        span.style = span.style.patch(sel);
                    }
                }
                lines.push(Line::from(spans));
            }
        }
        let reasons: Vec<Line<'static>> = all
            .iter()
            .filter_map(|a| {
                let why = disabled_reason(a.key, &self.view.gate)?;
                let text = ellipsize(
                    &format!("{DISABLED} {} {why}", a.key),
                    width.saturating_sub(1),
                );
                Some(Line::from(Span::styled(
                    format!(" {text}"),
                    Style::new().fg(t.text_muted),
                )))
            })
            .collect();
        if !reasons.is_empty() {
            lines.push(Line::default());
            lines.extend(reasons);
        }
        lines
    }

    fn milestone(&self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let block = panel("Milestone", Focus::Inactive, t);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let muted = Style::new().fg(t.text_muted);
        let width = inner.width as usize;
        let lines = match &self.view.milestone {
            MilestoneCard::Loading => vec![Line::styled(" Loading milestone…", muted)],
            MilestoneCard::NoMilestone => vec![
                Line::styled(" No milestone yet.", muted),
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled("m", key_style(t)),
                    Span::styled(" creates one", muted),
                ]),
            ],
            MilestoneCard::Active {
                title,
                closed,
                total,
            } => {
                let ratio = if *total == 0 {
                    0.0
                } else {
                    f64::from(*closed) / f64::from(*total)
                };
                let (filled, empty) = bar(ratio, width.saturating_sub(7), self.nerd_font);
                let done = Style::new().fg(t.accent_success);
                vec![
                    Line::styled(
                        format!(" {}", ellipsize(title, width.saturating_sub(1))),
                        Style::new().fg(t.text_primary).add_modifier(Modifier::BOLD),
                    ),
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(filled, done),
                        Span::styled(empty, muted),
                        Span::styled(format!(" {:>3.0}%", ratio * 100.0), done),
                    ]),
                    Line::from(vec![
                        Span::styled(format!(" {closed}"), done.add_modifier(Modifier::BOLD)),
                        Span::styled(" closed  ", muted),
                        Span::styled(
                            total.saturating_sub(*closed).to_string(),
                            Style::new()
                                .fg(t.accent_warning)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" open", muted),
                    ]),
                ]
            }
        };
        f.render_widget(Paragraph::new(lines), inner);
    }

    /// Wide: one gauge row per budget / quota window.
    fn budget_panel(&self, f: &mut Frame, area: Rect) {
        let block = panel("Budget & quota", Focus::Inactive, self.theme);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let width = inner.width as usize;
        let lines: Vec<Line> = self
            .view
            .budget
            .iter()
            .map(|r| {
                let value_w = r.value.chars().count().max(8);
                let bar_w = width.saturating_sub(1 + 11 + 1 + value_w);
                let mut spans = vec![Span::styled(
                    format!(" {:<11}", ellipsize(&r.label, 10)),
                    Style::new().fg(self.theme.text_muted),
                )];
                spans.extend(self.gauge(r, bar_w));
                spans.push(Span::styled(
                    format!(" {:>value_w$}", r.value),
                    self.level_style(r),
                ));
                Line::from(spans)
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    /// Regular / Compact: every row on one line.
    fn quota_strip(&self, f: &mut Frame, area: Rect) {
        let block = panel("Budget & quota", Focus::Inactive, self.theme);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut spans = Vec::new();
        for r in &self.view.budget {
            spans.push(Span::styled(
                format!(" {} ", r.label),
                Style::new().fg(self.theme.text_muted),
            ));
            spans.extend(self.gauge(r, 6));
            spans.push(Span::styled(format!(" {}  ", r.value), self.level_style(r)));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
    }

    fn gauge(&self, r: &QuotaRow, width: usize) -> Vec<Span<'static>> {
        match r.ratio {
            Some(ratio) => {
                let (filled, empty) = bar(ratio, width, self.nerd_font);
                vec![
                    Span::styled(filled, self.level_style(r)),
                    Span::styled(empty, Style::new().fg(self.theme.text_muted)),
                ]
            }
            None => vec![Span::raw(" ".repeat(width))],
        }
    }

    fn level_style(&self, r: &QuotaRow) -> Style {
        match r.ratio {
            Some(ratio) => gauge_level(ratio).style(self.theme),
            None => Style::new().fg(self.theme.text_primary),
        }
    }
}
