//! Quit + PR-linked lifecycle for the Interaction screen (#949, async
//! teardown #941).
//!
//! Since #949 (spec §4.4) a `/pushup` PR no longer closes the session:
//! [`InteractionScreen::on_pr_linked`] posts a `System` turn and the chat
//! stays open. The destructive worktree wipe (#740) runs ONLY on explicit
//! quit: [`InteractionScreen::begin_quit_teardown`] parks a
//! [`TeardownDispatch`] for the app layer, the wipe runs under
//! `tokio::task::spawn_blocking` (#941), and the outcome lands in
//! [`InteractionScreen::apply_teardown_result`], which terminates the view
//! and arms the 500ms auto-navigation timer.
//!
//! Two seams keep the flow testable without git or wall-clock sleeps:
//! [`WorktreeTeardownPort`] (over `wipe_worktree`) and [`Clock`] (over
//! `Instant::now`). Production wires `RealTeardown` + `RealClock`; tests
//! resolve the dispatch through `MockTeardown` + `FakeClock`.

use super::InteractionScreen;
use super::view_state::CloseReason;
use crate::session::interaction::{TurnRecord, TurnRole};
use crate::tui::screens::ScreenAction;
use crate::work::worktree_teardown::{TeardownError, wipe_worktree};
use chrono::Utc;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long the `Terminated` banner stays before auto-navigating back to the
/// Issues list. Any keypress short-circuits this (handled by the keymap).
pub(crate) const AUTO_NAV_DELAY: Duration = Duration::from_millis(500);

/// Everything the app layer needs to run one teardown off the UI thread
/// (#941). Produced by the terminator path, consumed by the app's
/// `spawn_blocking` dispatcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TeardownDispatch {
    pub(crate) issue_number: u64,
    pub(crate) path: std::path::PathBuf,
    pub(crate) branch: String,
    pub(crate) root: std::path::PathBuf,
}

/// Seam over [`wipe_worktree`] (#740) so the dispatcher can be tested without
/// touching git or the disk. Synchronous: teardown is blocking git I/O — it
/// runs under `spawn_blocking`, never on the UI thread (#941).
pub(crate) trait WorktreeTeardownPort {
    fn wipe(
        &self,
        issue_number: u64,
        path: &Path,
        branch: &str,
        worktree_root: &Path,
    ) -> Result<(), TeardownError>;
}

/// Production port: delegates straight to `wipe_worktree`.
pub(crate) struct RealTeardown;

impl WorktreeTeardownPort for RealTeardown {
    fn wipe(
        &self,
        issue_number: u64,
        path: &Path,
        branch: &str,
        worktree_root: &Path,
    ) -> Result<(), TeardownError> {
        wipe_worktree(issue_number, path, branch, worktree_root)
    }
}

/// Seam over `Instant::now` so the 500ms auto-nav timer is deterministic in
/// tests.
pub(crate) trait Clock {
    fn now(&self) -> Instant;
}

/// Production clock: real monotonic time.
pub(crate) struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

impl InteractionScreen {
    /// A `/pushup` PR was linked to this issue (#949, spec §4.4): keep the
    /// session open and return the activity-log action. The "PR #N" `System`
    /// turn is appended to the live session by the pipeline
    /// (`apply_pr_linked_announcement`) and read back through the view, so the
    /// screen no longer pushes it (#950 — was a double-write).
    pub(crate) fn on_pr_linked(&self, pr_number: u64) -> ScreenAction {
        super::activity_action(&crate::work::activity::InteractionActivity::PrLinked {
            issue: self.issue_number,
            pr_number,
        })
    }

    /// Start the quit teardown (#949): announce, then either finish
    /// synchronously (the no-worktree skip path — no git involved) or park
    /// a [`TeardownDispatch`] for the app layer and enter the in-flight
    /// state (#941). The blocking wipe itself never runs here;
    /// `Terminated(UserQuit)` lands in [`Self::apply_teardown_result`].
    pub(crate) fn begin_quit_teardown(&mut self) -> ScreenAction {
        // No trusted worktree root (cwd fallback in pool.rs) → there is no
        // isolated worktree to remove, and running the destructive teardown
        // with an untrusted root could target the main repo (#741 sec).
        // Close the session without wiping — nothing blocking, finish inline.
        if self.worktree_root.as_os_str().is_empty() {
            self.push_system_now("quitting — no isolated worktree to remove".to_string());
            let log = super::activity_action(
                &crate::work::activity::InteractionActivity::TeardownSkipped {
                    issue: self.issue_number,
                    why: "no isolated worktree".to_string(),
                },
            );
            return self.finalize_terminated(CloseReason::UserQuit, log);
        }

        self.push_system_now(format!(
            "quitting — wiping worktree {}…",
            self.worktree_path.display()
        ));
        self.teardown_in_flight = true;
        self.pending_teardown_dispatch = Some(TeardownDispatch {
            issue_number: self.issue_number,
            path: self.worktree_path.clone(),
            branch: self.branch.clone(),
            root: self.worktree_root.clone(),
        });
        // The TEARDOWN log line comes with the async result; the dispatch
        // site already logs the opening "INTERACTION closing" line.
        ScreenAction::None
    }

    /// Take the parked dispatch, if any. The app layer calls this right after
    /// driving the terminator (or applying a `TurnFinished` that drained one)
    /// and runs the wipe under `spawn_blocking` (#941).
    pub(crate) fn take_pending_teardown_dispatch(&mut self) -> Option<TeardownDispatch> {
        self.pending_teardown_dispatch.take()
    }

    /// True while a dispatched teardown has not resolved yet. Drives the
    /// "wiping worktree…" banner so the wait is visible, not a frozen
    /// frame, and locks the input so no turn can race the wipe (#949).
    pub(crate) fn is_teardown_in_flight(&self) -> bool {
        self.teardown_in_flight
    }

    /// Apply the async teardown outcome delivered by
    /// `TuiDataEvent::InteractionTeardownResult` (#941): append the
    /// success/failure `System` turn, set `close_reason`, terminate, and arm
    /// the auto-nav timer. Returns `ScreenAction::None` when no teardown was
    /// in flight (stale event).
    pub(crate) fn apply_teardown_result(&mut self, result: Result<(), String>) -> ScreenAction {
        if !self.teardown_in_flight {
            return ScreenAction::None;
        }
        self.teardown_in_flight = false;

        match result {
            Ok(()) => {
                self.push_system_now(format!(
                    "worktree removed at {}; branch {} deleted",
                    self.worktree_path.display(),
                    self.branch
                ));
                let log = super::activity_action(
                    &crate::work::activity::InteractionActivity::TeardownOk {
                        issue: self.issue_number,
                        path: self.worktree_path.clone(),
                    },
                );
                self.finalize_terminated(CloseReason::UserQuit, log)
            }
            Err(err) => {
                // git stderr can ride inside `err`; sanitize at ingestion (not
                // just at the renderer) so the error text is safe wherever it
                // flows — the System turn, `close_reason.tail`, and the log.
                let safe = crate::tui::screens::sanitize_for_terminal(&err);
                self.push_system_now(format!(
                    "worktree teardown failed: {safe}; manual cleanup: git worktree remove {}",
                    self.worktree_path.display()
                ));
                let log = super::activity_action(
                    &crate::work::activity::InteractionActivity::TeardownFail {
                        issue: self.issue_number,
                        path: self.worktree_path.clone(),
                        error: safe.clone(),
                    },
                );
                self.finalize_terminated(CloseReason::AgentFailure { tail: safe }, log)
            }
        }
    }

    /// Set the terminal state + close reason and arm the auto-nav timer. Shared
    /// by every `fire_terminator` exit so the transition is recorded once.
    fn finalize_terminated(&mut self, reason: CloseReason, action: ScreenAction) -> ScreenAction {
        self.close_reason = Some(reason);
        self.terminated = true;
        self.terminated_at = Some(self.clock.now());
        action
    }

    /// Append a finished `System` turn stamped now to the frozen view (#950).
    /// Only the quit-teardown epilogue uses this: by then the session is
    /// `Killed`, so the app has stopped refreshing the view and these turns
    /// survive on the terminal banner instead of being overwritten.
    fn push_system_now(&mut self, content: String) {
        let now = Utc::now();
        self.view.turns.push(TurnRecord {
            role: TurnRole::System,
            content,
            started_at: now,
            finished_at: Some(now),
        });
    }

    /// True once the `Terminated` banner has been shown for [`AUTO_NAV_DELAY`].
    /// The event loop pops back to the Issues list when this returns true.
    pub(crate) fn poll_auto_nav(&self) -> bool {
        matches!(
            self.terminated_at,
            Some(t) if self.clock.now().saturating_duration_since(t) >= AUTO_NAV_DELAY
        )
    }
}

#[cfg(test)]
#[path = "lifecycle_test_ports.rs"]
pub(crate) mod test_ports;
#[cfg(test)]
pub(crate) use test_ports::{FakeClock, MockTeardown};
