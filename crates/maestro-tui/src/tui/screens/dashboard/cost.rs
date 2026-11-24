//! Dashboard › Cost (§02.14, #1289): how much and how fast. KPI row
//! (spent, rate, time to cap, sessions), Spend by session over the work
//! list, and the Budget panel with what happens at each limit. Was the
//! `CostDashboard` screen (`cost_dashboard.rs`).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use super::usage::{
    self, SpendFacts, draw_budget, draw_kpis, draw_session_table, kpi_split, session_rows,
    short_model, spend_facts, time_to_cap,
};
use super::{Dashboard, DashboardView, session_cost};
use crate::session::types::Session;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::gauge::bar;
use crate::tui::ds::kpi_tile::{KpiState, KpiTile};
use crate::tui::ds::money::money;
use crate::tui::theme::Theme;

pub fn draw(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    let (kpis, body) = kpi_split(area);
    let facts = spend_facts(view);
    let texts = tile_texts(view, &facts);
    draw_kpis(f, kpis, &tiles(&texts), bp, theme, nerd_font);
    let (list, side) = match bp {
        Breakpoint::Wide | Breakpoint::Regular => {
            let w = usage::side_width(bp);
            let [l, s] =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(w)]).areas(body);
            (l, s)
        }
        _ => {
            let h = usage::budget_height(view).min(body.height / 2);
            let [l, s] = Layout::vertical([Constraint::Min(0), Constraint::Length(h)]).areas(body);
            (l, s)
        }
    };
    let rows = session_rows(view, dash);
    let per_session = view.usage.budget.as_ref().map(|b| b.per_session_usd);
    let spent = facts.spent;
    let now = view.now;
    let tail = |s: &Session, w: usize| {
        let mut t = spend_tail(s, w, spent, per_session, nerd_font);
        if w >= TIME_FROM {
            t.push_str(&format!(
                "{:>TIME_W$}",
                crate::tui::app::mission::elapsed(s, now)
            ));
        }
        t
    };
    let head = spend_head(list.width.saturating_sub(2) as usize);
    draw_session_table(
        f,
        list,
        "Spend by session",
        &head,
        &rows,
        &tail,
        theme,
        nerd_font,
    );
    draw_budget(f, side, view, theme, nerd_font);
}

/// Columns of the tail, widest first: cap, cost, share, model.
const MODEL_W: usize = 11;
const SHARE_W: usize = 7;
const COST_W: usize = 8;
const CAP_W: usize = 11;
/// Elapsed per session (the old Cost screen's column) from 100 columns.
const TIME_FROM: usize = 100;
const TIME_W: usize = 8;

fn spend_head(width: usize) -> String {
    let mut head = " ".to_string();
    if width >= 70 {
        head.push_str(&format!("{:<MODEL_W$}", "MODEL"));
    }
    if width >= 90 {
        head.push_str(&format!("{:<SHARE_W$}", "SHARE"));
    }
    head.push_str(&format!("{:>COST_W$}", "COST"));
    head.push_str(&format!("{:>CAP_W$}", "OF CAP"));
    if width >= TIME_FROM {
        head.push_str(&format!("{:>TIME_W$}", "TIME"));
    }
    head
}

/// `opus-4-5   ▓▓░░░░  $1.92  ▓▓░░░ 38%`: model, share of this run, cost
/// and the session against its own cap. Unpriced reads `$?` and `no cap`.
fn spend_tail(
    s: &Session,
    width: usize,
    spent: f64,
    per_session: Option<f64>,
    nerd_font: bool,
) -> String {
    let cost = session_cost(s);
    let mut tail = " ".to_string();
    if width >= 70 {
        tail.push_str(&format!("{:<MODEL_W$}", short_model(&s.model, MODEL_W - 1)));
    }
    if width >= 90 {
        let share = match cost {
            Some(c) if spent > 0.0 => c / spent,
            _ => 0.0,
        };
        let (full, empty) = bar(share, SHARE_W - 1, nerd_font);
        tail.push_str(&format!("{full}{empty} "));
    }
    tail.push_str(&format!("{:>COST_W$}", money(cost)));
    let cap = match (cost, per_session) {
        (None, _) => "no cap".to_string(),
        (Some(_), None) => "-".to_string(),
        (Some(c), Some(limit)) if limit > 0.0 => {
            let ratio = crate::budget::sanitize::sanitize_cost(c) / limit;
            let (full, empty) = bar(ratio, 5, nerd_font);
            format!("{full}{empty}{:>4.0}%", (ratio.min(9.99) * 100.0).round())
        }
        (Some(_), Some(_)) => "-".to_string(),
    };
    tail.push_str(&format!("{cap:>CAP_W$}"));
    tail
}

/// Owned text of the four Cost tiles; `tiles` borrows it.
struct TileTexts {
    spent: String,
    spent_ctx: String,
    rate: Option<String>,
    to_cap: Result<String, &'static str>,
    sessions: String,
    sessions_ctx: String,
}

fn tile_texts(view: &DashboardView<'_>, facts: &SpendFacts) -> TileTexts {
    let budget = view.usage.budget.as_ref();
    let spent_ctx = if facts.unpriced > 0 {
        format!("+ $? from {} unpriced", facts.unpriced)
    } else if let Some(b) = budget {
        format!("of {} cap", money(Some(b.total_usd)))
    } else {
        "no cap set".to_string()
    };
    let to_cap = match (budget, facts.per_hour) {
        (None, _) => Err("no cap set"),
        (Some(b), _) if b.total_usd > 0.0 && facts.spent >= b.total_usd => {
            Ok("reached".to_string())
        }
        (Some(b), Some(rate)) if rate > 0.0 => Ok(time_to_cap(b.total_usd - facts.spent, rate)),
        _ => Err("no spend yet"),
    };
    let live = view
        .sessions
        .iter()
        .filter(|s| !s.status.is_terminal())
        .count();
    TileTexts {
        spent: money(Some(facts.spent)),
        spent_ctx,
        rate: facts.per_hour.map(|r| format!("{}/h", money(Some(r)))),
        to_cap,
        sessions: format!("{live} live · {} done", view.sessions.len() - live),
        sessions_ctx: sessions_context(view, facts),
    }
}

/// `2 running · 1 failed · avg $1.60` (the old Cost screen's counts and
/// average; the average is over priced sessions).
fn sessions_context(view: &DashboardView<'_>, facts: &SpendFacts) -> String {
    use crate::session::types::SessionStatus::{Errored, FailedGates, Running};
    let count = |f: &dyn Fn(&Session) -> bool| view.sessions.iter().filter(|s| f(s)).count();
    let running = count(&|s| s.status == Running);
    let failed = count(&|s| matches!(s.status, Errored | FailedGates));
    let priced = view.sessions.len() - facts.unpriced;
    let mut ctx = format!("{running} running · {failed} failed");
    if priced > 0 {
        ctx.push_str(&format!(
            " · avg {}",
            money(Some(facts.spent / priced as f64))
        ));
    }
    ctx
}

fn tiles(t: &TileTexts) -> [KpiTile<'_>; 4] {
    let ready = usage::ready;
    [
        KpiTile {
            title: "Spent",
            state: ready(&t.spent, &t.spent_ctx),
        },
        KpiTile {
            title: "Rate",
            state: match &t.rate {
                Some(r) => ready(r, "avg this run"),
                None => KpiState::Empty {
                    reason: "under a minute in",
                },
            },
        },
        KpiTile {
            title: "To cap",
            state: match &t.to_cap {
                Ok(v) if v == "reached" => ready(v, "agents paused"),
                Ok(v) => ready(v, "at this rate"),
                Err(reason) => KpiState::Empty { reason },
            },
        },
        KpiTile {
            title: "Sessions",
            state: ready(&t.sessions, &t.sessions_ctx),
        },
    ]
}
