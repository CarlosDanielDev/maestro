//! Dashboard › Tokens layout (#1289): KPI row, Per-Provider Usage and
//! Tokens by session (`super`, the injection seam the snapshot tests use),
//! and the TurboQuant panel.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use super::{NoQuotaSnapshots, draw_token_dashboard_with_quota_and_badges};
use crate::budget::quota_snapshot::{MinimaxQuotaSnapshots, ProviderQuotaSnapshots};
use crate::session::types::TokenUsage;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::kpi_tile::{KpiState, KpiTile};
use crate::tui::ds::money::money;
use crate::tui::screens::dashboard::{DashboardView, usage};
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
    let spent = usage::spend_facts(view).spent;
    let total = format_tokens(k.total_tokens());
    let hit = format!("{:.0}%", k.cache_hit_ratio() * 100.0);
    // Measured first; the estimate is a ceiling and is never added to it.
    let (measured, m_usd) = savings_of(&items, SavingsKind::Actual);
    let (_, e_usd) = savings_of(&items, SavingsKind::Projection);
    let (tq, tq_ctx) = if measured > 0 {
        (
            money(Some(m_usd)),
            format!("measured · est. ≤ {}", money(Some(e_usd))),
        )
    } else {
        (money(Some(e_usd)), "est., a ceiling".to_string())
    };
    let spent = money(Some(spent));
    let ran = !view.sessions.is_empty();
    let ready = |value, context| KpiState::Ready {
        value,
        delta: None,
        spark: &[],
        context,
    };
    let none = KpiState::Empty {
        reason: "no runs yet",
    };
    let tiles = [
        KpiTile {
            title: "Tokens",
            state: if ran {
                ready(&total, "in, out, cache")
            } else {
                none
            },
        },
        KpiTile {
            title: "Cache hit",
            state: if ran { ready(&hit, "of input") } else { none },
        },
        KpiTile {
            title: "TurboQuant",
            state: if items.is_empty() {
                none
            } else {
                ready(&tq, &tq_ctx)
            },
        },
        KpiTile {
            title: "Spent",
            state: ready(&spent, usage::SCOPE_WORD),
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
    draw_token_dashboard_with_quota_and_badges(
        f,
        &view.sessions,
        view.usage.total_cost,
        quota.as_ref(),
        &view.usage.badges,
        left,
        theme,
    );
    let tq_view = (view.usage.tq_enabled, view.usage.tq_bit_width);
    draw_turboquant_panel(f, &items, tq_view, right, theme);
}
