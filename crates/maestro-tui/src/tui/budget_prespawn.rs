//! Pre-spawn budget gate modal (#776/#850).
//!
//! Renders an overlay modal when `BudgetEnforcer::check_pre_spawn` returns
//! `Warn` or `Block`. The orchestrator (input_handler.rs) reads the user's
//! single-letter chord (`y`/`n`/`s`) and acts accordingly.
//!
//! Hard rules (per architect blueprint §6):
//! - MUST use `theme.styled_block` for the border, NOT `theme.branding_bg`.
//! - Footer chord hints are exactly `[y]es [n]o [s]kip` — verbatim — on Warn;
//!   on Block only `[n]o` (#1243: `y`/`s` cannot spawn past the cap).
//! - Single-letter chords only; `Tab`/`BackTab`/`Up`/`Down`/`Enter`/`Esc`/`Ctrl+s`
//!   are reserved for outer Dashboard chord set and MUST NOT dismiss the modal.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::theme::Theme;

/// Render the pre-spawn budget gate modal at `area`. The caller (ui.rs)
/// is responsible for picking a centered sub-rect for the overlay.
///
/// `projected_total` is the sum of current global cost + projected next-turn
/// cost. `limit` is the configured `[budget].total_usd`. The modal renders
/// both side-by-side so the user understands the impending threshold.
pub fn draw_budget_prespawn(
    f: &mut Frame,
    projected_total: f64,
    limit: f64,
    area: Rect,
    theme: &Theme,
) {
    let block = theme.styled_block("Budget Alert", false);

    f.render_widget(Clear, area);

    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // intro
            Constraint::Length(1), // projected total
            Constraint::Length(1), // limit
            Constraint::Length(1), // chord footer
            Constraint::Min(0),
        ])
        .split(area);

    let exceeds = projected_total >= limit;
    let intro = if exceeds {
        Line::from(Span::styled(
            "Pre-spawn cost exceeds the configured budget.",
            Style::default()
                .fg(theme.gauge_high)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(
            "Pre-spawn cost crosses the budget alert threshold.",
            Style::default()
                .fg(theme.gauge_medium)
                .add_modifier(Modifier::BOLD),
        ))
    };

    let projected_line = Line::from(vec![
        Span::styled(
            "  Projected total: ",
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(
            format!("${:.2}", projected_total),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let limit_line = Line::from(vec![
        Span::styled(
            "  Limit:           ",
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(
            format!("${:.2}", limit),
            Style::default().fg(theme.text_primary),
        ),
    ]);
    // On Block `y`/`s` do nothing (#1243), so the footer offers only `n`.
    // `limit > 0` mirrors `check_pre_spawn`: no cap never blocks.
    let blocked = limit > 0.0 && exceeds;
    let footer = if blocked {
        Line::from(vec![
            Span::styled(
                "  [n]o ",
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "over the cap, cannot spawn",
                Style::default().fg(theme.text_secondary),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "  [y]es ",
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[n]o ",
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[s]kip",
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let intro_p = Paragraph::new(intro).alignment(Alignment::Left);
    let projected_p = Paragraph::new(projected_line).alignment(Alignment::Left);
    let limit_p = Paragraph::new(limit_line).alignment(Alignment::Left);
    let footer_p = Paragraph::new(footer).alignment(Alignment::Left);

    f.render_widget(block, area);
    f.render_widget(intro_p, inner_layout[0]);
    f.render_widget(projected_p, inner_layout[1]);
    f.render_widget(limit_p, inner_layout[2]);
    f.render_widget(footer_p, inner_layout[3]);
}

/// Pick a centered sub-rect for the modal. 50×8 fits the 5 visible lines
/// (intro + 1 + projected + limit + 1 + footer) + 1 line of breathing room
/// inside the styled_block. Falls back to the full `area` if too narrow.
pub fn modal_rect(area: Rect) -> Rect {
    let width = 50.min(area.width);
    let height = 8.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// The spend the shared launch dialog states before a spawn (#1262), read
/// from the same `[budget]` section that [`crate::tui::app::App::install_budget`]
/// gives the gate, so the dialog's projected `Block` is the gate's `Block`.
/// No config means no enforcer, so nothing is capped here either.
pub fn launch_budget(
    app: &crate::tui::app::App,
) -> crate::tui::widgets::launch_dialog::LaunchBudget {
    let mut budget = crate::tui::widgets::launch_dialog::LaunchBudget {
        // The model the launch really uses: the selected agent's, then the
        // default (`App::resolve_model_and_mode`; labels only pick the mode).
        model: app
            .resolve_model_and_mode(&[], Some(&app.selected_agent_id()))
            .0,
        spent: app.total_cost,
        ..Default::default()
    };
    if let Some(config) = app.config.as_ref() {
        budget.per_session_usd = config.budget.per_session_usd;
        budget.total_usd = config.budget.total_usd;
        budget.alert_threshold_pct = config.budget.alert_threshold_pct;
    }
    budget
}
