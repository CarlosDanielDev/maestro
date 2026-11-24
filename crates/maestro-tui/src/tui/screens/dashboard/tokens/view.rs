//! Dashboard › Tokens layout (#1289): KPI row, Per-Provider Usage (the
//! quota injection seam, `super::draw_rollup_top`), Tokens by session over
//! the Work list rows with the kept selection marked, and the TurboQuant
//! panel.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use super::{NoQuotaSnapshots, draw_rollup_top};
use crate::budget::quota_snapshot::{MinimaxQuotaSnapshots, ProviderQuotaSnapshots};
use crate::session::types::{Session, TokenUsage};
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::kpi_tile::{KpiState, KpiTile};
use crate::tui::ds::money::money;
use crate::tui::screens::dashboard::{Dashboard, DashboardView, session_cost, usage};
use crate::tui::theme::Theme;
use crate::tui::turboquant_dashboard::{classify_savings, draw_turboquant_panel, savings_of};
use crate::turboquant::adapter::{SavingsKind, TurboQuantAdapter};
use crate::util::formatting::format_tokens;

/// Dashboard › Tokens. The quota source is the live one when the App
/// holds it (`Usage::quota`), else `NoQuotaSnapshots` (cells read `-`).
pub fn draw(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    let (kpis, body) = usage::kpi_split(area);
    let adapter = TurboQuantAdapter::new(view.usage.tq_bit_width);
    let items = classify_savings(&view.sessions, &adapter);
    let mut k = TokenUsage::default();
    for s in &view.sessions {
        k.accumulate(&s.token_usage);
    }
    let total = format_tokens(k.total_tokens());
    let split = format!(
        "in {} · out {} · cache {}",
        format_tokens(k.input_tokens),
        format_tokens(k.output_tokens),
        format_tokens(k.cache_read_tokens + k.cache_creation_tokens)
    );
    let hit = format!("{:.0}%", k.cache_hit_ratio() * 100.0);
    // Measured first; the estimate is a ceiling and is never added to it.
    // The words match the TurboQuant panel's (`would save` while off).
    let (measured, m_usd) = savings_of(&items, SavingsKind::Actual);
    let (_, e_usd) = savings_of(&items, SavingsKind::Projection);
    let estimate_word = if view.usage.tq_enabled {
        "a ceiling"
    } else {
        "would save"
    };
    let (tq, tq_ctx) = if measured > 0 {
        let ctx = format!("measured · est. {} {estimate_word}", money(Some(e_usd)));
        (money(Some(m_usd)), ctx)
    } else {
        (money(Some(e_usd)), format!("est., {estimate_word}"))
    };
    let spent = money(Some(usage::spend_facts(view).spent));
    let ran = !view.sessions.is_empty();
    let none = KpiState::Empty {
        reason: "no runs yet",
    };
    let tiles = [
        KpiTile {
            title: "Tokens",
            state: if ran {
                usage::ready(&total, &split)
            } else {
                none
            },
        },
        KpiTile {
            title: "Cache hit",
            state: if ran {
                usage::ready(&hit, "of input")
            } else {
                none
            },
        },
        KpiTile {
            title: "TurboQuant",
            state: if items.is_empty() {
                none
            } else {
                usage::ready(&tq, &tq_ctx)
            },
        },
        KpiTile {
            title: "Spent",
            state: usage::ready(&spent, usage::SCOPE_WORD),
        },
    ];
    usage::draw_kpis(f, kpis, &tiles, bp, theme, nerd_font);
    let [left, right] = match bp {
        Breakpoint::Wide | Breakpoint::Regular => {
            let w = usage::side_width(bp);
            Layout::horizontal([Constraint::Min(0), Constraint::Length(w)]).areas(body)
        }
        _ => Layout::vertical([Constraint::Min(0), Constraint::Length(8)]).areas(body),
    };
    let quota: Box<dyn ProviderQuotaSnapshots> = match &view.usage.quota {
        Some(q) => Box::new(MinimaxQuotaSnapshots::new(q.clone())),
        None => Box::new(NoQuotaSnapshots),
    };
    let badges = &view.usage.badges;
    let list = draw_rollup_top(f, &view.sessions, quota.as_ref(), badges, left, theme);
    let rows = usage::session_rows(view, dash);
    let head = token_head(list.width.saturating_sub(2) as usize);
    usage::draw_session_table(
        f,
        list,
        "Tokens by session",
        &head,
        &rows,
        &token_tail,
        theme,
        nerd_font,
    );
    let tq_view = (view.usage.tq_enabled, view.usage.tq_bit_width);
    draw_turboquant_panel(f, &items, tq_view, right, theme);
}

/// Cache W and hit % from 80 columns; in, out, cache R and `$/MTok` always.
const FULL_W: usize = 80;

fn token_head(width: usize) -> String {
    let mut head = format!(" {:>7}{:>7}{:>8}", "IN", "OUT", "CACHE R");
    if width >= FULL_W {
        head.push_str(&format!("{:>8}{:>5}", "CACHE W", "HIT"));
    }
    head.push_str(&format!("{:>9}", "$/MTOK"));
    head
}

/// `  8.0k   9.0k  375.0k   30.0k  98%   $4.55`; `$?` when unpriced.
fn token_tail(s: &Session, width: usize) -> String {
    let k = &s.token_usage;
    let mut tail = format!(
        " {:>7}{:>7}{:>8}",
        format_tokens(k.input_tokens),
        format_tokens(k.output_tokens),
        format_tokens(k.cache_read_tokens)
    );
    if width >= FULL_W {
        tail.push_str(&format!(
            "{:>8}{:>5}",
            format_tokens(k.cache_creation_tokens),
            format!("{:.0}%", k.cache_hit_ratio() * 100.0)
        ));
    }
    // Per million tokens, the price table's unit.
    let per_mtok = session_cost(s).map(|c| k.cost_per_kilo_token(c.max(0.0)) * 1000.0);
    tail.push_str(&format!("{:>9}", money(per_mtok)));
    tail
}
