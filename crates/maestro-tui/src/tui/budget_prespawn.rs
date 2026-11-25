//! Pre-spawn budget gate modal (#776/#850, §02.24 #1276).
//!
//! Opens when `BudgetEnforcer::check_pre_spawn` returns `Warn` or `Block`.
//! It names the session, states the money (SPEND rows: spent, next turn,
//! after, worst case, left) and what happens at the cap, with a
//! `BLOCK` / `WARN` chip. Keys ([`chord`]): `S` spawns (Warn only: locked
//! on Block), `b` raises the cap in Settings › Budget, `n` drops the
//! session from the queue. Every other key stays in the modal.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::ds::gauge::{bar, gauge_level};
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::theme::Theme;

/// The gate's keys: `S` spawn, `b` raise cap, `n` drop. Spawning spends
/// money, so it is uppercase only; `y` / `s` (the old twins) are gone.
pub fn chord(key: &KeyEvent) -> Option<char> {
    match key.code {
        KeyCode::Char('S') => Some('S'),
        KeyCode::Char('b') => Some('b'),
        KeyCode::Char('n') => Some('n'),
        _ => None,
    }
}

/// What the gate states, from the same enforcer and projector as
/// [`crate::tui::app::App::pre_spawn_decision`].
#[derive(Debug, Clone, PartialEq)]
pub struct GateView {
    /// `#415` or `S-1a2b3c4d`.
    pub session: String,
    pub spent: f64,
    /// The projected next turn (`DefaultBudgetProjector`).
    pub next: f64,
    /// `[budget].per_session_usd`: what one session may spend.
    pub per_session: f64,
    /// `[budget].total_usd`.
    pub cap: f64,
    /// The gate's own `Block`: `S` does nothing.
    pub blocked: bool,
    /// The gate now says `Allow` (e.g. `b` raised the cap): no warning
    /// applies any more.
    pub fits: bool,
}

impl GateView {
    pub fn after(&self) -> f64 {
        self.spent + self.next
    }

    pub fn worst(&self) -> f64 {
        self.spent + self.per_session
    }

    pub fn left(&self) -> f64 {
        (self.cap - self.spent).max(0.0)
    }
}

/// The gate for the parked session `session_id`.
pub fn gate_view(app: &crate::tui::app::App, session_id: uuid::Uuid) -> GateView {
    let session = app.pool.get_session(session_id);
    let next = session
        .zip(app.budget_projector.as_ref())
        .map(|(s, p)| p.projected_turn_cost(s))
        .unwrap_or(0.0);
    use crate::budget::PreSpawnDecision;
    let decision = session.map(|s| app.pre_spawn_decision(s));
    let blocked = matches!(decision, Some(PreSpawnDecision::Block { .. }));
    let fits = matches!(decision, Some(PreSpawnDecision::Allow));
    // The enforcer the gate decides with, so the rows match its Block.
    let enforcer = app.budget_enforcer.as_ref();
    GateView {
        session: session
            .map(crate::tui::app::helpers::session_label)
            .unwrap_or_else(|| "this session".to_string()),
        spent: app.total_cost,
        next,
        per_session: enforcer.map_or(0.0, |e| e.per_session_limit()),
        cap: enforcer.map_or(0.0, |e| e.total_limit()),
        blocked,
        fits,
    }
}

/// Draw the gate for `session_id` centred in `area` (ui.rs, also over the
/// SizeGuard).
pub fn draw_gate(
    f: &mut Frame,
    app: &crate::tui::app::App,
    session_id: uuid::Uuid,
    area: Rect,
    theme: &Theme,
) {
    let cap_what = app
        .config
        .as_ref()
        .map(|c| crate::tui::screens::dashboard::usage::budget_thresholds(&c.budget)[2].what)
        .unwrap_or("all agents pause");
    draw(
        f,
        &gate_view(app, session_id),
        cap_what,
        modal_rect(area),
        theme,
    );
}

/// Draw `view` in `area`. `cap_what` is the Usage "What happens" line for
/// the cap, word for word.
pub fn draw(f: &mut Frame, view: &GateView, cap_what: &str, area: Rect, theme: &Theme) {
    let (focus, chip, chip_color) = if view.blocked {
        (Focus::Danger, " BLOCK ", theme.accent_error)
    } else if view.fits {
        (Focus::Overlay, " OK ", theme.accent_success)
    } else {
        (Focus::Overlay, " WARN ", theme.accent_warning)
    };
    let block = panel("Budget gate", focus, theme);
    f.render_widget(Clear, area);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let inner = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );

    let muted = Style::default().fg(theme.text_muted);
    let text = Style::default().fg(theme.text_primary);
    let bold = text.add_modifier(Modifier::BOLD);
    let label = |s: &str| Span::styled(format!("{s:<8}"), muted);
    let outcome = if view.blocked {
        "would pass the budget cap".to_string()
    } else if view.fits {
        "now fits the budget".to_string()
    } else if view.cap > 0.0 {
        format!(
            "would use {:.0}% of the budget",
            view.after() / view.cap * 100.0
        )
    } else {
        "crosses the budget alert".to_string()
    };
    let ratio = if view.cap > 0.0 {
        view.spent / view.cap
    } else {
        0.0
    };
    let level = gauge_level(ratio).style(theme);
    let (filled, empty) = bar(ratio, 12, crate::icon_mode::use_nerd_font());
    let worst_note = if view.cap > 0.0 && view.worst() >= view.cap {
        Span::styled(
            "  ! passes the cap",
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("  under the cap", muted)
    };
    let spawn = if view.blocked {
        Span::styled("S spawn (locked: over the cap)", muted)
    } else {
        Span::styled("S spawn now", bold)
    };
    let key = |k: &str| {
        Span::styled(
            k.to_string(),
            Style::default()
                .fg(theme.keybind_key)
                .add_modifier(Modifier::BOLD),
        )
    };
    // Below 80x24 the SizeGuard hides Settings, so `b` is not offered
    // (input_handler ignores it there too).
    let mut keys_row = vec![spawn];
    if !crate::tui::ds::size_guard::is_shown(f.area()) {
        keys_row.extend([Span::raw("   "), key("b"), Span::styled(" raise cap", text)]);
    }
    keys_row.extend([Span::raw("   "), key("n"), Span::styled(" drop", text)]);
    let lines = vec![
        Line::from(vec![
            Span::styled(
                chip,
                Style::default()
                    .fg(chip_color)
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            ),
            Span::styled(format!(" {} ", view.session), bold),
            Span::styled(outcome, text),
        ]),
        Line::from(""),
        Line::from(Span::styled("SPEND", bold)),
        Line::from(vec![
            label("spent"),
            Span::styled(filled, level),
            Span::styled(empty, Style::default().fg(theme.gauge_background)),
            Span::styled(
                format!(" {} of {}", money(Some(view.spent)), money(Some(view.cap))),
                text,
            ),
            Span::styled(format!(" · {} left", money(Some(view.left()))), muted),
        ]),
        Line::from(vec![
            label("next"),
            Span::styled(money(Some(view.next)), text),
            Span::styled(" est. for the next turn", muted),
        ]),
        Line::from(vec![
            label("after"),
            Span::styled(money(Some(view.after())), text),
        ]),
        Line::from(vec![
            label("worst"),
            Span::styled(money(Some(view.worst())), text),
            Span::styled(
                format!(" = spent + one {} session", money(Some(view.per_session))),
                muted,
            ),
            worst_note,
        ]),
        Line::from(""),
        Line::from(vec![
            label("at cap"),
            Span::styled(format!("{cap_what}; new spawns wait"), text),
        ]),
        Line::from(""),
        Line::from(keys_row),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

/// Pick a centered sub-rect for the modal: 66×13 holds the 11 rows of
/// [`draw`] inside the border. Shrinks to `area` when it is smaller.
pub fn modal_rect(area: Rect) -> Rect {
    let width = 66.min(area.width);
    let height = 13.min(area.height);
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
