//! The Plan's cost line (#1290, §02.17 "Cost line"): the exact worst case
//! (`run_plan`'s issues in waves × per-session cap) against the budget left,
//! in the launch dialog's words. No estimate without run history: the
//! prompt-token figure is never shown.

use super::types::PlanPreview;
use crate::tui::ds::gauge::{bar, gauge_level};
use crate::tui::ds::money::money;
use crate::tui::theme::Theme;
use crate::tui::widgets::launch_dialog::LaunchBudget;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// What the cost line states, before styling.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cost {
    /// `None` when there is no per-session cap: no bound exists.
    pub worst_case: Option<f64>,
    /// `None` when there is no total cap.
    pub left: Option<f64>,
    /// `(spent + worst case) / total`, with a total cap.
    pub ratio: Option<f64>,
}

impl Cost {
    pub(super) fn of(plan: &PlanPreview, b: &LaunchBudget) -> Self {
        // The launch dialog's bound (sessions × per-session cap), read from
        // this frame's budget; `run_plan`'s `worst_case_usd` is the same
        // product once the plan saw the cap.
        let worst_case =
            (b.per_session_usd > 0.0).then_some(plan.final_count as f64 * b.per_session_usd);
        let capped = b.total_usd > 0.0;
        Self {
            worst_case,
            left: capped.then(|| (b.total_usd - b.spent).max(0.0)),
            ratio: worst_case
                .filter(|_| capped)
                .map(|w| (b.spent + w) / b.total_usd),
        }
    }

    /// The worst case is more than the budget left: the cap stops the run.
    pub(super) fn over(&self) -> bool {
        matches!((self.worst_case, self.left), (Some(w), Some(l)) if w > l)
    }
}

fn muted(theme: &Theme) -> Style {
    Style::default().fg(theme.text_secondary)
}

pub(super) fn cost_lines(
    plan: &PlanPreview,
    b: &LaunchBudget,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let cost = Cost::of(plan, b);
    let n = plan.final_count;
    let worst = match (cost.worst_case, cost.ratio) {
        (None, _) => Line::from(vec![
            Span::styled("worst case ", muted(theme)),
            Span::styled("unknown", Style::default().fg(theme.accent_warning)),
            Span::styled("  ·  no per-session cap set", muted(theme)),
        ]),
        (Some(w), None) => Line::from(vec![
            Span::styled("worst case ", muted(theme)),
            Span::styled(money(Some(w)), Style::default().fg(theme.text_primary)),
            Span::styled(
                format!(
                    " ({n} × {} cap)  ·  no total cap",
                    money(Some(b.per_session_usd))
                ),
                muted(theme),
            ),
        ]),
        (Some(w), Some(ratio)) => {
            let level = gauge_level(ratio).style(theme);
            let (filled, empty) = bar(ratio, 16, crate::icon_mode::use_nerd_font());
            Line::from(vec![
                Span::styled("worst case ", muted(theme)),
                Span::styled(filled, level),
                Span::styled(empty, Style::default().fg(theme.gauge_background)),
                Span::styled(
                    format!(" {}", money(Some(w))),
                    level.add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" ({n} × {} cap)", money(Some(b.per_session_usd))),
                    muted(theme),
                ),
                Span::styled(format!("  ·  {} left now", money(cost.left)), muted(theme)),
            ])
        }
    };
    let mut lines = vec![worst];
    if cost.over() {
        lines.push(Line::from(Span::styled(
            "! worst case is over the budget left · the total cap can stop this run",
            Style::default().fg(theme.accent_warning),
        )));
    }
    lines.push(Line::from(vec![
        Span::styled("estimate ", muted(theme)),
        Span::styled("not available", Style::default().fg(theme.text_muted)),
        Span::styled("  ·  no run history for this team", muted(theme)),
    ]));
    lines
}
