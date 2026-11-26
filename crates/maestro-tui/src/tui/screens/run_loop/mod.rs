//! The run loop (#1287, design `docs/design-system/index.html` §02.22 "Run
//! loop"): `QueueExecution` and `ContinuousPause` draw one screen, the run
//! plan now live. A Run order panel with a state per row, one decision card
//! when the loop needs you, and a Run bar with progress, spend against the
//! approved worst case, `h` hold and `S` stop.
//!
//! Presentation and controls only: the queue still runs one item at a time
//! (`work::executor`). The screen reads the executor or the continuous state
//! every frame ([`RunView::from_app`]); its own state is the picked option
//! and the stop dialog ([`RunLoopState`]).

pub mod draw;
pub mod input;
mod view;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_safety;

pub use draw::draw;
pub use view::{Flavour, Phase, Row, RowState, RunView};

use crate::tui::ds::confirm::TypedConfirm;
use crate::tui::ds::money::money;

/// What the run loop keeps between frames.
#[derive(Default)]
pub struct RunLoopState {
    /// The option picked on one issue's decision card: `(issue, option)`.
    /// Any other card opens on its default (option 0, never a spend).
    pub choice: Option<(u64, usize)>,
    /// Stop › Kill came before the running item's session existed (its
    /// issue was still being fetched): that session never starts.
    pub kill_unstarted: Option<u64>,
    /// `S`: the "Stop the run?" dialog, open.
    pub stop: Option<StopDialog>,
}

impl RunLoopState {
    /// The picked option on `issue`'s card; 0 (the safe default) when the
    /// pick was made on another card or never.
    pub fn choice_for(&self, issue: u64) -> usize {
        match self.choice {
            Some((n, i)) if n == issue => i,
            _ => 0,
        }
    }
}

/// "Stop the run?" (`S`): finish the running session (default) or kill it.
/// Both are typed confirms: `stop` to drain, the running count to kill.
pub struct StopDialog {
    pub kill: bool,
    running: usize,
    /// The running item has no session yet (its issue is being fetched).
    starting: bool,
    not_started: usize,
    /// The most the running sessions can still cost (`None`: no cap).
    most: Option<f64>,
    /// What Kill does to the running session's work (#1456,
    /// `App::kill_teardown_rows`): empty while it has no session yet.
    kill_rows: Vec<(String, String)>,
    pub confirm: TypedConfirm,
}

impl StopDialog {
    pub fn new(running: usize, starting: bool, not_started: usize, most: Option<f64>) -> Self {
        let mut dialog = Self {
            kill: false,
            running,
            starting,
            not_started,
            most,
            kill_rows: Vec::new(),
            confirm: TypedConfirm::new("", "", "", vec![]),
        };
        dialog.confirm = dialog.confirm();
        dialog
    }

    /// The dialog with what Kill does to the running session's work.
    pub fn with_kill_rows(mut self, kill_rows: Vec<(String, String)>) -> Self {
        self.kill_rows = kill_rows;
        self.confirm = self.confirm();
        self
    }

    /// `↑↓`: switch between drain and kill. Kill needs something running.
    /// The typed text resets, so a token typed for one never confirms the
    /// other.
    pub fn toggle(&mut self) {
        let kill = !self.kill && self.running > 0;
        if kill != self.kill {
            self.kill = kill;
            self.confirm = self.confirm();
        }
    }

    fn confirm(&self) -> TypedConfirm {
        let (kill, running, not_started) = (self.kill, self.running, self.not_started);
        let mark = |on: bool| if on { "▸ ●" } else { "  ○" };
        let mut rows = vec![
            (
                String::new(),
                format!("{} Drain: the {running} running finish", mark(!kill)),
            ),
            (
                String::new(),
                if self.starting {
                    format!("{} Kill the {running} starting: it never runs", mark(kill))
                } else if running > 0 {
                    format!("{} Kill the {running} running now (as X does)", mark(kill))
                } else {
                    "  ○ Kill: nothing is running".to_string()
                },
            ),
            (
                "both".to_string(),
                "nothing new starts · ↑↓ picks".to_string(),
            ),
        ];
        if let Some(m) = self.most.filter(|_| !kill) {
            rows.push((
                "cost".to_string(),
                format!("at most {} more", money(Some(m))),
            ));
        }
        // #1456: say what the teardown does. `pool.finalize` deletes a
        // completed or killed session's worktree and keeps the worktree of
        // a FailedGates or Errored one.
        if kill {
            rows.extend(self.kill_rows.iter().cloned());
        } else if running > 0 {
            rows.push((
                "gone".to_string(),
                "its worktree, once it completes".to_string(),
            ));
        }
        rows.extend([
            (
                "kept".to_string(),
                "opened PRs, labels written, failed runs' worktrees".to_string(),
            ),
            (
                "open".to_string(),
                format!("{not_started} not started stay open on GitHub"),
            ),
        ]);
        let (token, action) = if kill {
            (running.to_string(), "kill and stop")
        } else {
            ("stop".to_string(), "stop the run")
        };
        TypedConfirm::new("Stop the run?", token, action, rows)
    }
}
