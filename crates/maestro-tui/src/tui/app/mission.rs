//! App side of Home Mission Control (#1264): build the plain
//! [`MissionView`] from App state. Layout and keys live in
//! `tui::screens::home::mission`.

use chrono::{DateTime, Utc};

use super::App;
use crate::session::types::Session;
use crate::tui::ds::fleet_table::FleetRow;
use crate::tui::ds::money::money;
use crate::tui::screens::home::ProjectStats;
use crate::tui::screens::home::mission::{ActionGate, MilestoneCard, MissionView, QuotaRow};
use crate::tui::screens::sanitize_for_terminal;

impl App {
    /// Everything Home draws, read at `now`.
    pub fn mission_view(&self, now: DateTime<Utc>) -> MissionView {
        let sessions = self.pool.all_sessions();
        MissionView {
            attention: self.attention(),
            fleet_busy: sessions.iter().any(|s| !s.status.is_terminal()),
            fleet: sessions.iter().map(|s| fleet_row(s, now)).collect(),
            budget: self.budget_rows(),
            milestone: self.milestone_card(),
            gate: self.action_gate(),
            now,
        }
    }

    /// What decides whether a Home action can run now.
    pub fn action_gate(&self) -> ActionGate {
        ActionGate {
            gh_auth_ok: self.gh_auth_ok,
            has_milestone: self.loaded_stats().map(|s| s.milestone_active.is_some()),
        }
    }

    /// Project stats once they loaded; `None` while they load.
    fn loaded_stats(&self) -> Option<&ProjectStats> {
        let home = self.screen_state.home_screen.as_ref()?;
        Some(&home.stats).filter(|s| s.loaded)
    }

    fn milestone_card(&self) -> MilestoneCard {
        let Some(stats) = self.loaded_stats() else {
            return MilestoneCard::Loading;
        };
        match &stats.milestone_active {
            Some(m) => MilestoneCard::Active {
                title: sanitize_for_terminal(&m.title),
                closed: m.closed,
                total: m.total,
            },
            None => MilestoneCard::NoMilestone,
        }
    }

    /// Spend against the global cap, then every fresh quota window.
    fn budget_rows(&self) -> Vec<QuotaRow> {
        let cap = self.budget_enforcer.as_ref().map(|e| e.total_limit());
        let spend = QuotaRow {
            label: "spend".to_string(),
            ratio: cap.filter(|c| *c > 0.0).map(|c| self.total_cost / c),
            value: match cap {
                Some(c) => format!("{}/{}", money(Some(self.total_cost)), money(Some(c))),
                None => money(Some(self.total_cost)),
            },
        };
        let mut rows = vec![spend];
        if !self.quota.enabled {
            return rows;
        }
        let mut ids: Vec<&String> = self.quota.windows.keys().collect();
        ids.sort();
        for id in ids {
            let w = &self.quota.windows[id];
            if w.stale {
                continue;
            }
            let provider = sanitize_for_terminal(id);
            for (name, window) in [
                ("5h", &w.five_hour),
                ("7d", &w.seven_day),
                ("7d sonnet", &w.seven_day_sonnet),
            ] {
                if let Some(q) = window {
                    rows.push(QuotaRow {
                        label: format!("{provider} {name}"),
                        ratio: Some(q.utilization_pct / 100.0),
                        value: format!("{:.0}%", q.utilization_pct),
                    });
                }
            }
        }
        rows
    }
}

fn fleet_row(s: &Session, now: DateTime<Utc>) -> FleetRow {
    FleetRow {
        status: s.status,
        issue: s.issue_number,
        title: s.issue_title.clone().unwrap_or_else(|| s.prompt.clone()),
        model: s.model.clone(),
        context: s.context_pct,
        // Same `$?` rule as `Session::cost_display`.
        cost: (!(s.cost_unknown && s.cost_usd <= 0.0)).then_some(s.cost_usd),
        elapsed: elapsed(s, now),
    }
}

/// `12m03s` like `Session::elapsed_display`, but at `now` so it is testable.
fn elapsed(s: &Session, now: DateTime<Utc>) -> String {
    let Some(start) = s.started_at else {
        return "—".to_string();
    };
    let secs = (s.finished_at.unwrap_or(now) - start).num_seconds().max(0);
    let (m, sec) = (secs / 60, secs % 60);
    if m > 0 {
        format!("{m}m{sec:02}s")
    } else {
        format!("{sec}s")
    }
}
