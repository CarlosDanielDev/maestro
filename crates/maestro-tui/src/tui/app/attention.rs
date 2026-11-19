//! App side of the Needs-you queue (#1259): build the plain
//! [`AttentionInput`] from App state, and keep the terminal title in step
//! with the count. Ranking and drawing live in `tui::ds::attention`.

use std::io::Write;

use super::App;
use crate::budget::{BudgetAction, BudgetCheck};
use crate::tui::ds::attention::{
    AttentionInput, AttentionItem, BudgetSignal, QuotaSignal, SessionSignal, exit_title, rank,
    terminal_title,
};

/// A provider's busiest window at or above this shows as a quota notice
/// (the `High` gauge level).
const QUOTA_NOTICE_PCT: f64 = 80.0;

impl App {
    /// Plain snapshot of everything the Needs-you queue reads.
    pub fn attention_input(&self) -> AttentionInput {
        let sessions = self
            .pool
            .all_sessions()
            .into_iter()
            .map(|s| SessionSignal {
                id: s.id,
                issue: s.issue_number,
                title: s.issue_title.clone(),
                status: s.status,
                since: s.finished_at.or(s.started_at),
            })
            .collect();
        let doctor_failures = self
            .screen_state
            .home_screen
            .as_ref()
            .map(|h| h.warnings.clone())
            .unwrap_or_default();
        let budget =
            self.budget_enforcer
                .as_ref()
                .and_then(|e| match e.check_global(self.total_cost) {
                    BudgetAction::Ok => None,
                    BudgetAction::Alert(_) | BudgetAction::Kill => Some(BudgetSignal {
                        spent: self.total_cost,
                        cap: e.total_limit(),
                        paused: self.pool.active_paused_count(),
                    }),
                });
        AttentionInput {
            sessions,
            doctor_failures,
            gh_auth_lost: !self.gh_auth_ok,
            budget,
            quota: self.quota_notices(),
        }
    }

    /// The ranked Needs-you queue.
    pub fn attention(&self) -> Vec<AttentionItem> {
        rank(&self.attention_input())
    }

    /// Write `(n) maestro` to the terminal title when it changed since the
    /// last call. The first write saves the shell's title on the terminal's
    /// title stack first (XTWINOPS 22), so quit can restore it. Errors from
    /// the writer are returned, not logged.
    pub fn sync_terminal_title<W: Write>(&mut self, out: &mut W) -> std::io::Result<()> {
        let title = terminal_title(self.attention().len());
        if self.last_terminal_title.as_deref() == Some(title.as_str()) {
            return Ok(());
        }
        if self.last_terminal_title.is_none() {
            out.write_all(PUSH_TITLE)?;
        }
        crossterm::execute!(out, crossterm::terminal::SetTitle(&title))?;
        self.last_terminal_title = Some(title);
        Ok(())
    }

    fn quota_notices(&self) -> Vec<QuotaSignal> {
        if !self.quota.enabled {
            return Vec::new();
        }
        let mut ids: Vec<&String> = self.quota.windows.keys().collect();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| {
                let w = &self.quota.windows[id];
                if w.stale {
                    return None;
                }
                let busiest = [&w.five_hour, &w.seven_day, &w.seven_day_sonnet]
                    .into_iter()
                    .flatten()
                    .map(|q| q.utilization_pct)
                    .reduce(f64::max)?;
                (busiest >= QUOTA_NOTICE_PCT).then(|| QuotaSignal {
                    provider: id.clone(),
                    used_pct: busiest,
                })
            })
            .collect()
    }
}

/// XTWINOPS: push / pop the window title on the terminal's title stack.
/// Terminals without the stack ignore them.
const PUSH_TITLE: &[u8] = b"\x1b[22;0t";
const POP_TITLE: &[u8] = b"\x1b[23;0t";

/// Quit restore: write the plain `exit_title()`, then pop the title saved
/// by the first [`App::sync_terminal_title`]. Where the terminal has a
/// title stack the shell's own title comes back; elsewhere the pop is
/// ignored and the tab keeps `maestro`, with no stale `(n)` count.
///
/// Best effort: the quit path ignores the result on purpose. The terminal
/// is being torn down and a lost title must never block or fail quit.
pub(crate) fn write_exit_title<W: Write>(out: &mut W) -> std::io::Result<()> {
    crossterm::execute!(out, crossterm::terminal::SetTitle(exit_title()))?;
    out.write_all(POP_TITLE)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use crate::session::types::SessionStatus;
    use crate::tui::snapshot_tests::make_session;

    fn title_after_sync(app: &mut super::App) -> String {
        let mut out = Vec::new();
        app.sync_terminal_title(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn terminal_title_counts_attention() {
        let mut app = crate::tui::make_test_app("maestro-1259-title");
        assert_eq!(
            title_after_sync(&mut app),
            "\u{1b}[22;0t\u{1b}]0;maestro\u{7}",
            "the first write saves the shell's title first"
        );

        app.pool
            .enqueue(make_session(SessionStatus::FailedGates, Some(412)));
        app.pool
            .enqueue(make_session(SessionStatus::Running, Some(409)));
        app.pool
            .enqueue(make_session(SessionStatus::NeedsPr, Some(405)));
        assert_eq!(app.attention().len(), 2);
        assert_eq!(title_after_sync(&mut app), "\u{1b}]0;(2) maestro\u{7}");
    }

    #[test]
    fn exit_restore_writes_the_plain_title_then_pops_the_saved_one() {
        // `maestro` first: a terminal without the title stack ignores the
        // pop and keeps it; one with the stack restores the shell's title.
        let mut out = Vec::new();
        super::write_exit_title(&mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\u{1b}]0;maestro\u{7}\u{1b}[23;0t"
        );
    }

    #[test]
    fn title_is_saved_once_per_run() {
        let mut app = crate::tui::make_test_app("maestro-1382-title-push");
        assert!(title_after_sync(&mut app).starts_with("\u{1b}[22;0t"));
        app.pool
            .enqueue(make_session(SessionStatus::FailedGates, Some(412)));
        assert_eq!(
            title_after_sync(&mut app),
            "\u{1b}]0;(1) maestro\u{7}",
            "later writes do not push again"
        );
    }

    #[test]
    fn terminal_title_is_written_only_when_it_changes() {
        let mut app = crate::tui::make_test_app("maestro-1259-title-once");
        assert!(!title_after_sync(&mut app).is_empty());
        assert_eq!(title_after_sync(&mut app), "", "same title: no write");
    }
}
