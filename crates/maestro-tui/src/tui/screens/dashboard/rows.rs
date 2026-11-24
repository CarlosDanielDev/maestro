//! The Dashboard work list (§02.1 `dashboard::work_list`): three groups,
//! one selection. Rows are keys, not indexes, so a session that finishes
//! between the look and the key does not move the target.

use crate::provider::github::types::PendingPr;
use crate::session::types::{Session, SessionStatus};

/// A group header of the work list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Sessions,
    PrPipeline,
    Finished,
}

/// One row of the work list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkKey {
    Session(uuid::Uuid),
    /// A restored `PendingPr`, by issue number.
    PendingPr(u64),
    /// An open PR under CI polling, by PR number.
    OpenPr(u64),
    Header(Group),
}

impl WorkKey {
    pub fn is_header(self) -> bool {
        matches!(self, WorkKey::Header(_))
    }
}

/// The rows in draw order and the selected row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkList {
    pub rows: Vec<WorkKey>,
    pub selected: Option<WorkKey>,
}

/// Finished work leaves the Sessions group. FailedGates is terminal but
/// still actionable (re-run gates), so it stays in Sessions.
fn is_finished(s: &Session) -> bool {
    matches!(s.status, SessionStatus::Completed | SessionStatus::Killed)
}

impl WorkList {
    /// Sessions (pool order), PR pipeline (open PRs by number, then the
    /// restored pending PRs), Finished · this run. The selection keeps its
    /// key when the key is still listed, else moves to the first row.
    pub fn rebuild(&mut self, sessions: &[&Session], open_prs: &[u64], pending: &[PendingPr]) {
        let mut rows = vec![WorkKey::Header(Group::Sessions)];
        rows.extend(
            sessions
                .iter()
                .filter(|s| !is_finished(s))
                .map(|s| WorkKey::Session(s.id)),
        );
        rows.push(WorkKey::Header(Group::PrPipeline));
        let mut open: Vec<u64> = open_prs.to_vec();
        open.sort_unstable();
        rows.extend(open.into_iter().map(WorkKey::OpenPr));
        rows.extend(pending.iter().map(|p| WorkKey::PendingPr(p.issue_number)));
        rows.push(WorkKey::Header(Group::Finished));
        rows.extend(
            sessions
                .iter()
                .filter(|s| is_finished(s))
                .map(|s| WorkKey::Session(s.id)),
        );
        if !self.selected.is_some_and(|k| rows.contains(&k)) {
            self.selected = rows.iter().copied().find(|k| !k.is_header());
        }
        self.rows = rows;
    }

    /// Select `key` when it is a listed row.
    pub fn select(&mut self, key: WorkKey) {
        if !key.is_header() && self.rows.contains(&key) {
            self.selected = Some(key);
        }
    }

    /// Next selectable row down; headers are skipped. Stops at the end.
    pub fn move_down(&mut self) {
        self.step(true);
    }

    /// Next selectable row up; headers are skipped. Stops at the top.
    pub fn move_up(&mut self) {
        self.step(false);
    }

    fn step(&mut self, down: bool) {
        self.step_where(down, |k| !k.is_header());
    }

    /// `move_down` / `move_up` over session rows only: the Cost and Tokens
    /// views list no PR rows, so their keys never land on one (#1289).
    pub fn move_session(&mut self, down: bool) {
        self.step_where(down, |k| matches!(k, WorkKey::Session(_)));
    }

    /// Move a PR-row selection to the first session row, or to nothing
    /// when there is no session (a Usage view shows no PR rows).
    pub fn keep_on_session(&mut self) {
        if self.selected_session().is_none() {
            self.selected = self
                .rows
                .iter()
                .copied()
                .find(|k| matches!(k, WorkKey::Session(_)));
        }
    }

    fn step_where(&mut self, down: bool, stop: impl Fn(&WorkKey) -> bool) {
        let Some(at) = self
            .selected
            .and_then(|k| self.rows.iter().position(|r| *r == k))
        else {
            return;
        };
        let next = if down {
            self.rows[at + 1..].iter().find(|k| stop(k))
        } else {
            self.rows[..at].iter().rev().find(|k| stop(k))
        };
        if let Some(k) = next {
            self.selected = Some(*k);
        }
    }

    /// The selected session's id, when a session row is selected.
    pub fn selected_session(&self) -> Option<uuid::Uuid> {
        match self.selected {
            Some(WorkKey::Session(id)) => Some(id),
            _ => None,
        }
    }

    /// The rows under `group`'s header.
    pub fn group_rows(&self, group: Group) -> &[WorkKey] {
        let Some(start) = self.rows.iter().position(|k| *k == WorkKey::Header(group)) else {
            return &[];
        };
        let rest = &self.rows[start + 1..];
        let end = rest
            .iter()
            .position(|k| k.is_header())
            .unwrap_or(rest.len());
        &rest[..end]
    }
}

/// (current step, failed step) on the `run ─ gates ─ push ─ PR ─ CI`
/// stepper. Step 5 means every step is done.
pub fn pipeline_stage(status: SessionStatus) -> (usize, Option<usize>) {
    use SessionStatus::*;
    match status {
        Queued | Spawning | Running | Retrying | Stalled | Paused | NeedsReview | Interactive => {
            (0, None)
        }
        GatesRunning => (1, None),
        FailedGates => (1, Some(1)),
        NeedsPr => (3, None),
        CiFix | ConflictFix => (4, Some(4)),
        Completed => (5, None),
        Errored | Killed => (0, Some(0)),
    }
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
