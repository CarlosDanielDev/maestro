//! 80% global budget alert banner (#776/#850).
//!
//! Top chrome row that renders only when `BudgetEnforcer::check_global` returns
//! `Alert(pct)` or `Kill`. No-op when `enforcer` is `None` or when the action
//! is `Ok` — keeps the chrome row blank under normal operation.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::budget::{BudgetAction, BudgetCheck, BudgetEnforcer};
use crate::tui::theme::Theme;

/// Render the budget banner inside `area` if and only if the enforcer
/// reports `Alert(pct)` or `Kill` for `total_cost`. Otherwise renders
/// nothing (the area stays blank). Returns true if the banner was drawn.
pub fn draw_budget_banner_if_alerting(
    f: &mut Frame,
    enforcer: Option<&BudgetEnforcer>,
    total_cost: f64,
    area: Rect,
    theme: &Theme,
) -> bool {
    let Some(enforcer) = enforcer else {
        return false;
    };
    let (label, colour) = match enforcer.check_global(total_cost) {
        BudgetAction::Ok => return false,
        BudgetAction::Alert(pct) => (
            format!(
                "  BUDGET ALERT: {}% of ${:.2} used (${:.2} so far)  ",
                pct,
                enforcer.total_limit(),
                total_cost
            ),
            theme.gauge_medium,
        ),
        BudgetAction::Kill => (
            format!(
                "  BUDGET EXCEEDED: ${:.2} of ${:.2}  ",
                total_cost,
                enforcer.total_limit()
            ),
            theme.gauge_high,
        ),
    };

    let line = Line::from(vec![Span::styled(
        label,
        Style::default().fg(colour).add_modifier(Modifier::BOLD),
    )]);
    let paragraph = Paragraph::new(line).alignment(Alignment::Left);
    f.render_widget(paragraph, area);
    true
}
