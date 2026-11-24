//! Usage views of the Dashboard (§02.14 of `docs/design-system/index.html`,
//! #1289): `c` Cost and `t` Tokens next to `o` Ops. They share the work
//! list selection, the scope line (this run) and the Budget rules. The old
//! `CostDashboard` and `TokenDashboard` modes are aliases that land here
//! (`alias_view`); the TurboQuant screen is gone (a tile and a panel in
//! Tokens; its ⌃K entry opens Tokens). Every dollar goes through
//! `ds::money`, every gauge through `ds::gauge`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::rows::{Group, WorkKey};
use super::{Dashboard, DashboardView, session_cost};
use crate::config::BudgetConfig;
use crate::session::types::Session;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::gauge::{bar, gauge_level};
use crate::tui::ds::kpi_tile::{KPI_TILE_HEIGHT, KpiState, KpiTile, kpi_tile_for_mode};
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::theme::Theme;

/// Which Dashboard view draws; kept in `Dashboard::view`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Ops,
    Cost,
    Tokens,
}

/// The view an old Usage mode opens. These modes are aliases now.
pub fn alias_view(mode: TuiMode) -> Option<View> {
    match mode {
        TuiMode::CostDashboard => Some(View::Cost),
        TuiMode::TokenDashboard => Some(View::Tokens),
        _ => None,
    }
}

/// `mode`, or `TuiMode::Dashboard` with `dash` on its view when `mode` is
/// an alias. `App::navigate_to` and `route_alias` both go through it.
pub fn resolve(mode: TuiMode, dash: &mut Dashboard) -> TuiMode {
    match alias_view(mode) {
        Some(view) => {
            dash.view = view;
            TuiMode::Dashboard
        }
        None => mode,
    }
}

/// Where `App::navigate_to(mode)` lands: an alias opens its view, the
/// Dashboard opens on Ops. Going back (nav stack) keeps the last view.
pub fn navigate_target(mode: TuiMode, dash: &mut Dashboard) -> TuiMode {
    if mode == TuiMode::Dashboard {
        dash.view = View::Ops;
    }
    resolve(mode, dash)
}

/// Turn an alias mode set straight on `tui_mode` into the Dashboard view.
pub fn route_alias(app: &mut App) {
    app.tui_mode = resolve(app.tui_mode, &mut app.screen_state.dashboard);
}

/// Width of the right column (Budget, TurboQuant) on Wide and Regular:
/// the design's 58 and 48.
pub fn side_width(bp: Breakpoint) -> u16 {
    if bp == Breakpoint::Wide { 58 } else { 48 }
}

/// The App numbers the Usage views read, copied for one draw.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// `App::total_cost`, the number the global cap checks.
    pub total_cost: f64,
    /// `[budget]`; `None` when maestro.toml has none (no cap applies).
    pub budget: Option<BudgetConfig>,
    /// `App::start_time`: the scope of every number (this run).
    pub since: chrono::DateTime<chrono::Utc>,
    pub tq_enabled: bool,
    pub tq_bit_width: u8,
    /// Live quota badges (`App::quota_badges`).
    pub badges: Vec<String>,
    /// The live quota source for Per-Provider Usage (`App::minimax_quota`).
    pub quota: Option<std::sync::Arc<crate::agent_provider::minimax::quota::MinimaxQuota>>,
}

/// Every Usage number counts this process's run (`Usage::since`).
pub const SCOPE_WORD: &str = "this run";

/// One line of the Budget panel's "What happens" list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Threshold {
    pub at_usd: f64,
    pub label: &'static str,
    pub what: &'static str,
}

/// What happens at each limit, from `cfg` so it never drifts from the
/// config. The words follow `App::check_budget` (`app/budget.rs`).
pub fn budget_thresholds(cfg: &BudgetConfig) -> [Threshold; 3] {
    [
        Threshold {
            at_usd: cfg.total_usd * f64::from(cfg.alert_threshold_pct) / 100.0,
            label: "alert",
            what: "banner + log line",
        },
        Threshold {
            at_usd: cfg.per_session_usd,
            label: "one session",
            what: "it stops (errored)",
        },
        Threshold {
            at_usd: cfg.total_usd,
            label: "the cap",
            what: "all agents pause",
        },
    ]
}

/// `this run · since 10:00 UTC` (UTC so every machine reads the same).
pub fn scope_line(usage: &Usage) -> String {
    format!("this run · since {} UTC", usage.since.format("%H:%M"))
}

/// The sessions of the work list in its order (Sessions, then Finished),
/// with the group headers; PR rows are not Usage rows.
pub fn session_rows<'a>(view: &'a DashboardView<'_>, dash: &Dashboard) -> Vec<UsageRow<'a>> {
    let mut out = Vec::new();
    for key in &dash.list.rows {
        match *key {
            WorkKey::Header(g @ (Group::Sessions | Group::Finished)) => {
                let n = dash.list.group_rows(g).len();
                if n > 0 {
                    out.push(UsageRow::Header(g, n));
                }
            }
            WorkKey::Session(id) => {
                if let Some(s) = view.session(id) {
                    let selected = dash.list.selected == Some(*key);
                    out.push(UsageRow::Session(s, selected));
                }
            }
            _ => {}
        }
    }
    out
}

pub enum UsageRow<'a> {
    Header(Group, usize),
    Session(&'a Session, bool),
}

/// `Spent`, `+ $? from 1 unpriced` and the rate pieces the KPI tiles share.
pub struct SpendFacts {
    pub spent: f64,
    pub unpriced: usize,
    /// Dollars per hour over this run; `None` before a minute has passed.
    pub per_hour: Option<f64>,
}

pub fn spend_facts(view: &DashboardView<'_>) -> SpendFacts {
    let spent = crate::budget::sanitize::sanitize_cost(view.usage.total_cost);
    let unpriced = view
        .sessions
        .iter()
        .filter(|s| session_cost(s).is_none())
        .count();
    let minutes = (view.now - view.usage.since).num_minutes();
    let per_hour = (minutes >= 1).then(|| spent * 60.0 / minutes as f64);
    SpendFacts {
        spent,
        unpriced,
        per_hour,
    }
}

/// `2h 10m` until the cap at this run's rate.
pub fn time_to_cap(left_usd: f64, per_hour: f64) -> String {
    let minutes = (left_usd / per_hour * 60.0).round() as i64;
    if minutes >= 60 {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

/// A KPI tile with a value and one context line (no delta, no sparkline).
pub fn ready<'a>(value: &'a str, context: &'a str) -> KpiState<'a> {
    KpiState::Ready {
        value,
        delta: None,
        spark: &[],
        context,
    }
}

/// Draw up to `bp`'s tile count (Wide 4, Regular 3, Compact 2) in a row.
pub fn draw_kpis(
    f: &mut Frame,
    area: Rect,
    tiles: &[KpiTile],
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    let n = match bp {
        Breakpoint::Wide => 4,
        Breakpoint::Regular => 3,
        _ => 2,
    }
    .min(tiles.len());
    let rects = Layout::horizontal(vec![Constraint::Fill(1); n]).split(area);
    for (tile, rect) in tiles.iter().zip(rects.iter()) {
        kpi_tile_for_mode(f, *rect, tile, theme, nerd_font);
    }
}

/// Split a Usage view: KPI row on top, then the body.
pub fn kpi_split(area: Rect) -> (Rect, Rect) {
    let [kpis, body] =
        Layout::vertical([Constraint::Length(KPI_TILE_HEIGHT), Constraint::Min(0)]).areas(area);
    (kpis, body)
}

/// Rows the Budget panel needs with its border (see `draw_budget`).
pub fn budget_height(view: &DashboardView<'_>) -> u16 {
    match view.usage.budget {
        None => 4,
        Some(_) if spend_facts(view).unpriced > 0 => 10,
        Some(_) => 9,
    }
}

/// The Budget panel: spent against the cap on a `ds::gauge` bar, then what
/// happens at the alert, the per-session cap and the total cap.
pub fn draw_budget(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    theme: &Theme,
    nerd_font: bool,
) {
    let block = panel("Budget · this run", Focus::Inactive, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let muted = Style::new().fg(theme.text_muted);
    let text = Style::new().fg(theme.text_primary);
    let Some(cfg) = &view.usage.budget else {
        let lines = vec![
            Line::from(Span::styled(" No [budget] in maestro.toml.", text)),
            Line::from(Span::styled(" No cap applies: nothing stops spend.", muted)),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    };
    let facts = spend_facts(view);
    let ratio = if cfg.total_usd > 0.0 {
        facts.spent / cfg.total_usd
    } else {
        0.0
    };
    let level = gauge_level(ratio).style(theme);
    let width = (inner.width as usize).saturating_sub(16).min(24);
    let (full, empty) = bar(ratio, width, nerd_font);
    let pct = (ratio.clamp(0.0, 9.99) * 100.0).round();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" Spent ", muted),
            Span::styled(money(Some(facts.spent)), text.add_modifier(Modifier::BOLD)),
            Span::styled(format!(" of {}", money(Some(cfg.total_usd))), muted),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(full, level),
            Span::styled(empty, muted),
            Span::styled(format!(" {pct:.0}%"), level),
        ]),
        Line::from(Span::styled(
            " What happens",
            muted.add_modifier(Modifier::BOLD),
        )),
    ];
    for t in budget_thresholds(cfg) {
        // The per-session line is per session, so it is never "now" here.
        let now = t.label != "one session" && t.at_usd > 0.0 && facts.spent >= t.at_usd;
        let mut spans = vec![
            Span::styled(format!(" {:>8}", money(Some(t.at_usd))), text),
            Span::styled(format!(" {:<11} ", t.label), muted),
            Span::styled(t.what, text),
        ];
        if now {
            spans.push(Span::styled(" ← now", level));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        " At the cap new spawns wait too.",
        muted,
    )));
    if facts.unpriced > 0 {
        lines.push(Line::from(Span::styled(
            format!(" {} unpriced: $? and no cap", facts.unpriced),
            Style::new().fg(theme.accent_warning),
        )));
    }
    let lines = lines
        .into_iter()
        .map(|l| crate::tui::ds::fit::ellipsize_line(l, inner.width as usize))
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), inner);
}

/// The body of a Usage list before any run: what to do, not a blank table.
pub fn empty_lines(theme: &Theme) -> Vec<Line<'static>> {
    let muted = Style::new().fg(theme.text_muted);
    vec![
        Line::from(Span::styled(
            " ○ Nothing has run yet",
            Style::new().fg(theme.text_primary),
        )),
        Line::from(Span::styled(
            "   numbers appear here when a session starts",
            muted,
        )),
        Line::from(vec![
            Span::styled("   start work from ", muted),
            Span::styled("1", key_style(theme)),
            Span::styled(" Home", muted),
        ]),
    ]
}

/// A Usage table over the work list sessions: the Ops row (chip, issue,
/// title) with `tail` on the right, the shared selection marked `▌`, and
/// the empty state before any run. `head` is the column header of `tail`.
#[allow(clippy::too_many_arguments)]
pub fn draw_session_table(
    f: &mut Frame,
    area: Rect,
    title: &str,
    head: &str,
    rows: &[UsageRow<'_>],
    tail: &dyn Fn(&Session, usize) -> String,
    theme: &Theme,
    nerd_font: bool,
) {
    let block = panel(title, Focus::Active, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    if rows.is_empty() {
        f.render_widget(Paragraph::new(empty_lines(theme)), inner);
        return;
    }
    let muted = Style::new().fg(theme.text_muted);
    // Rows end one cell short of the edge (`mark` + `row_line`).
    let pad = width.saturating_sub(1 + head.chars().count());
    let mut lines = vec![Line::from(Span::styled(
        format!("{}{head}", " ".repeat(pad)),
        muted.add_modifier(Modifier::BOLD),
    ))];
    let mut selected_line = None;
    for row in rows {
        match row {
            UsageRow::Header(g, n) => {
                lines.push(super::draw::header_line(*g, *n, theme));
            }
            UsageRow::Session(s, selected) => {
                let chip =
                    crate::tui::ds::status_chip::status_chip_for_mode(s.status, theme, nerd_font);
                let t = tail(s, width);
                let title = super::session_title(s);
                let line = super::draw::row_line(chip, s.issue_number, &title, t, width, theme);
                if *selected && selected_line.is_none() {
                    selected_line = Some(lines.len());
                }
                lines.push(super::draw::mark(line, *selected, width, theme));
            }
        }
    }
    let height = inner.height as usize;
    let offset = selected_line.map_or(0, |at| (at + 1).saturating_sub(height));
    let shown: Vec<Line> = lines.into_iter().skip(offset).take(height).collect();
    f.render_widget(Paragraph::new(shown), inner);
}

/// `opus-4-5` from `claude-opus-4-5`, cut to `w`.
pub fn short_model(model: &str, w: usize) -> String {
    let m = model.strip_prefix("claude-").unwrap_or(model);
    crate::tui::screens::landing::draw::ellipsize(m, w)
}
