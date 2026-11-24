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
    pub confirm: TypedConfirm,
}

impl StopDialog {
    pub fn new(running: usize, starting: bool, not_started: usize, most: Option<f64>) -> Self {
        let confirm = Self::confirm(false, running, starting, not_started, most);
        Self {
            kill: false,
            running,
            starting,
            not_started,
            most,
            confirm,
        }
    }

    /// `↑↓`: switch between drain and kill. Kill needs something running.
    /// The typed text resets, so a token typed for one never confirms the
    /// other.
    pub fn toggle(&mut self) {
        let kill = !self.kill && self.running > 0;
        if kill != self.kill {
            self.kill = kill;
            self.confirm = Self::confirm(
                kill,
                self.running,
                self.starting,
                self.not_started,
                self.most,
            );
        }
    }

    fn confirm(
        kill: bool,
        running: usize,
        starting: bool,
        not_started: usize,
        most: Option<f64>,
    ) -> TypedConfirm {
        let mark = |on: bool| if on { "▸ ●" } else { "  ○" };
        let mut rows = vec![
            (
                String::new(),
                format!("{} Drain: the {running} running finish", mark(!kill)),
            ),
            (
                String::new(),
                if starting {
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
        if let Some(m) = most.filter(|_| !kill) {
            rows.push((
                "cost".to_string(),
                format!("at most {} more", money(Some(m))),
            ));
        }
        rows.extend([
            (
                "kept".to_string(),
                "opened PRs, labels already written, worktrees".to_string(),
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
