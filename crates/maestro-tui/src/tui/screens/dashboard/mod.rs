//! Dashboard, the operations cockpit (§02.1 of
//! `docs/design-system/index.html`, #1280): the work list (Sessions · PR
//! pipeline · Finished) with the selected item's detail beside it.
//! `TuiMode::Overview` is an alias that draws this screen
//! (`ui::route_overview_alias`). Keys: `dispatch`; layout: `draw`.

pub mod detail;
pub mod dispatch;
pub mod draw;
pub mod rows;

use std::collections::HashMap;
use std::time::Instant;

use crate::provider::github::types::{PendingPr, PendingPrStatus};
use crate::provider::types::CheckRun;
use crate::session::types::Session;
use crate::tui::ds::armed::ArmedEnter;
use rows::{WorkKey, WorkList};

/// Dashboard state kept in `ScreenState::dashboard`.
#[derive(Debug, Default)]
pub struct Dashboard {
    pub list: WorkList,
    /// The armed `⏎` of a pending PR row; `armed_for` names the row.
    pub armed: ArmedEnter,
    pub armed_for: Option<u64>,
    /// Tests inject the clock; the app uses `Instant::now`.
    pub clock: Option<Instant>,
}

impl Dashboard {
    pub fn now(&self) -> Instant {
        self.clock.unwrap_or_else(Instant::now)
    }

    /// Rebuild the rows from the app data.
    pub fn rebuild(&mut self, view: &DashboardView<'_>) {
        let open: Vec<u64> = view.ci.keys().copied().collect();
        self.list.rebuild(&view.sessions, &open, view.pending);
        if self.armed_for.is_some() && self.list.selected != self.armed_for.map(WorkKey::PendingPr)
        {
            self.disarm();
        }
    }

    pub fn disarm(&mut self) {
        self.armed.reset();
        self.armed_for = None;
    }
}

/// The App data the Dashboard reads, borrowed for one draw or key.
pub struct DashboardView<'a> {
    pub sessions: Vec<&'a Session>,
    pub pending: &'a [PendingPr],
    pub ci: &'a HashMap<u64, Vec<CheckRun>>,
    pub gh_auth_ok: bool,
    pub now: chrono::DateTime<chrono::Utc>,
}

impl DashboardView<'_> {
    pub fn pending_pr(&self, issue: u64) -> Option<&PendingPr> {
        self.pending.iter().find(|p| p.issue_number == issue)
    }

    pub fn session(&self, id: uuid::Uuid) -> Option<&Session> {
        self.sessions.iter().copied().find(|s| s.id == id)
    }
}

/// Why `⏎` on this pending PR does nothing, or `None` when it may arm.
/// A signed-out gh does not lock the row: `gh_auth_ok` turns true again
/// only after a gh call works, and this retry is that call.
pub fn pr_lock_reason(pending: &PendingPr) -> Option<&'static str> {
    match pending.status {
        PendingPrStatus::PermanentlyFailed => {
            Some("retries stopped · the same error repeated; re-run the issue from Home")
        }
        // A create is running or queued: a second one would race it.
        PendingPrStatus::Retrying => Some("a gh pr create is running for this PR"),
        PendingPrStatus::RetryScheduled => Some("a retry is already queued for this PR"),
        PendingPrStatus::AwaitingManualRetry => None,
    }
}

/// Issue title, or the prompt when the session has no issue.
fn session_title(s: &Session) -> String {
    s.issue_title.clone().unwrap_or_else(|| s.prompt.clone())
}

/// `None` (`$?`) for an unpriced model, the `Session::cost_display` rule.
fn session_cost(s: &Session) -> Option<f64> {
    (!(s.cost_unknown && s.cost_usd <= 0.0)).then_some(s.cost_usd)
}

/// The exact write a pending PR row makes on GitHub.
pub fn pr_write_description(pending: &PendingPr) -> String {
    format!(
        "gh pr create --head {} --base {} (title from issue #{})",
        pending.branch, pending.base_branch, pending.issue_number
    )
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod dispatch_tests;

#[cfg(test)]
#[path = "reach_tests.rs"]
mod reach_tests;
