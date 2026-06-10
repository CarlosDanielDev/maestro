//! Terminator UI flow for the Interaction screen (#741, async teardown #941).
//!
//! Bridges #739's `InteractionLifecycleEvent::PrLinkedToIssue` to #740's
//! `wipe_worktree`: appends `System` turns announcing the close, dispatches
//! the worktree teardown OFF the UI thread, applies the result when it comes
//! back, sets the terminal `close_reason`, and arms a 500ms auto-navigation
//! timer.
//!
//! The flow is split in two (#941) so blocking `git` never runs on the UI
//! thread:
//! 1. the terminator path appends the opening `System` turn, flips the
//!    in-flight flag, and parks a [`TeardownDispatch`] for the app layer;
//! 2. the app runs the wipe under `tokio::task::spawn_blocking` and delivers
//!    the outcome back as `TuiDataEvent::InteractionTeardownResult`, which
//!    lands in [`InteractionScreen::apply_teardown_result`].
//!
//! Two seams keep the flow testable without git or wall-clock sleeps:
//! [`WorktreeTeardownPort`] (over `wipe_worktree`) and [`Clock`] (over
//! `Instant::now`). Production wires `RealTeardown` + `RealClock`; tests
//! resolve the dispatch through `MockTeardown` + `FakeClock`.

use super::{InteractionScreen, LOG_TAG};
use crate::session::interaction::{CloseReason, InteractionState, TurnRecord, TurnRole};
use crate::session::interaction_lifecycle::InteractionLifecycleEvent;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::ScreenAction;
use crate::work::worktree_teardown::{TeardownError, wipe_worktree};
use chrono::Utc;
use std::path::Path;
use std::time::{Duration, Instant};

/// Activity-log tag for the teardown result line (distinct from `LOG_TAG`).
const TEARDOWN_TAG: &str = "TEARDOWN";

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
    /// Handle a terminator signal for this session.
    ///
    /// - `Idle`: fire now (append turns, run teardown, terminate).
    /// - `Streaming`: queue it; [`Self::drain_queued_terminator`] fires it once
    ///   the in-flight turn settles back to `Idle`.
    /// - `Terminated`: the session was already closed. If the user pre-closed
    ///   via `Ctrl+W`, log that teardown is skipped (#741 race contract);
    ///   otherwise it is an idempotent no-op.
    ///
    /// Returns the `TEARDOWN` (or skip) activity-log action for the caller to
    /// surface. The opening `INTERACTION closing` line is emitted at the
    /// dispatch site.
    pub(crate) fn on_terminator_signaled(
        &mut self,
        event: InteractionLifecycleEvent,
    ) -> Option<ScreenAction> {
        match self.state {
            InteractionState::Terminated => {
                if matches!(self.close_reason, Some(CloseReason::UserQuit)) {
                    return Some(ScreenAction::LogActivity {
                        tag: LOG_TAG.to_string(),
                        message: format!(
                            "#{} session pre-closed by user; teardown skipped",
                            self.issue_number
                        ),
                        level: LogLevel::Info,
                    });
                }
                None
            }
            InteractionState::Streaming => {
                self.queued_terminator = Some(event);
                None
            }
            InteractionState::Idle => Some(self.fire_terminator(event)),
        }
    }

    /// Fire a terminator that was deferred during `Streaming`. Called from the
    /// `TurnFinished` arm once the turn settles to `Idle`. Returns the teardown
    /// activity-log action, or `None` when nothing was queued.
    pub(crate) fn drain_queued_terminator(&mut self) -> Option<ScreenAction> {
        let event = self.queued_terminator.take()?;
        Some(self.fire_terminator(event))
    }

    /// Start the teardown flow: announce and either finish synchronously (the
    /// no-worktree skip path — no git involved) or park a
    /// [`TeardownDispatch`] for the app layer and enter the in-flight state
    /// (#941). The blocking wipe itself never runs here.
    fn fire_terminator(&mut self, event: InteractionLifecycleEvent) -> ScreenAction {
        let InteractionLifecycleEvent::PrLinkedToIssue { pr_number, .. } = event else {
            return ScreenAction::None;
        };

        // No trusted worktree root (cwd fallback in pool.rs) → there is no
        // isolated worktree to remove, and running the destructive teardown
        // with an untrusted root could target the main repo (#741 sec). Close
        // the session without wiping — nothing blocking, so finish inline.
        if self.worktree_root.as_os_str().is_empty() {
            self.push_system_now(format!(
                "PR #{pr_number} created → finishing session (no isolated worktree to remove)"
            ));
            let log = self.teardown_log(
                format!(
                    "#{} no isolated worktree; teardown skipped",
                    self.issue_number
                ),
                LogLevel::Info,
            );
            return self.finalize_terminated(CloseReason::PrCreated { pr_number }, log);
        }

        self.push_system_now(format!(
            "PR #{pr_number} created → finishing session and wiping worktree…"
        ));
        self.teardown_pr_in_flight = Some(pr_number);
        self.pending_teardown_dispatch = Some(TeardownDispatch {
            issue_number: self.issue_number,
            path: self.worktree_path.clone(),
            branch: self.branch.clone(),
            root: self.worktree_root.clone(),
        });
        // The TEARDOWN log line comes with the async result; the dispatch site
        // already logs the opening "INTERACTION closing" line.
        ScreenAction::None
    }

    /// Take the parked dispatch, if any. The app layer calls this right after
    /// driving the terminator (or applying a `TurnFinished` that drained one)
    /// and runs the wipe under `spawn_blocking` (#941).
    pub(crate) fn take_pending_teardown_dispatch(&mut self) -> Option<TeardownDispatch> {
        self.pending_teardown_dispatch.take()
    }

    /// True while a dispatched teardown has not resolved yet. Drives the
    /// "wiping worktree…" banner so the wait is visible, not a frozen frame.
    pub(crate) fn is_teardown_in_flight(&self) -> bool {
        self.teardown_pr_in_flight.is_some()
    }

    /// Apply the async teardown outcome delivered by
    /// `TuiDataEvent::InteractionTeardownResult` (#941): append the
    /// success/failure `System` turn, set `close_reason`, terminate, and arm
    /// the auto-nav timer — the same end state the old synchronous path
    /// produced. Returns `ScreenAction::None` when no teardown was in flight
    /// (stale event).
    pub(crate) fn apply_teardown_result(&mut self, result: Result<(), String>) -> ScreenAction {
        let Some(pr_number) = self.teardown_pr_in_flight.take() else {
            return ScreenAction::None;
        };

        match result {
            Ok(()) => {
                self.push_system_now(format!(
                    "worktree removed at {}; branch {} deleted",
                    self.worktree_path.display(),
                    self.branch
                ));
                let log = self.teardown_log(
                    format!(
                        "#{} worktree removed at {}; branch deleted",
                        self.issue_number,
                        self.worktree_path.display()
                    ),
                    LogLevel::Info,
                );
                self.finalize_terminated(CloseReason::PrCreated { pr_number }, log)
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
                let log = self.teardown_log(
                    format!(
                        "#{} worktree teardown FAILED: {}; worktree kept at {}",
                        self.issue_number,
                        safe,
                        self.worktree_path.display()
                    ),
                    LogLevel::Warn,
                );
                self.finalize_terminated(CloseReason::AgentFailure { tail: safe }, log)
            }
        }
    }

    /// Build a `TEARDOWN`-tagged activity-log action. One constructor for all
    /// three `fire_terminator` exits (skip / removed / failed).
    fn teardown_log(&self, message: String, level: LogLevel) -> ScreenAction {
        ScreenAction::LogActivity {
            tag: TEARDOWN_TAG.to_string(),
            message,
            level,
        }
    }

    /// Set the terminal state + close reason and arm the auto-nav timer. Shared
    /// by every `fire_terminator` exit so the transition is recorded once.
    fn finalize_terminated(&mut self, reason: CloseReason, action: ScreenAction) -> ScreenAction {
        self.close_reason = Some(reason);
        self.state = InteractionState::Terminated;
        self.terminated_at = Some(self.clock.now());
        action
    }

    /// Append a finished `System` turn stamped now.
    fn push_system_now(&mut self, content: String) {
        let now = Utc::now();
        self.history.push(TurnRecord {
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
use std::cell::{Cell, RefCell};
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::rc::Rc;

/// Canned [`WorktreeTeardownPort`] for tests. Records each call and returns a
/// pre-seeded outcome on the first call (`take`-once), then `Ok(())`.
#[cfg(test)]
pub(crate) struct MockTeardown {
    canned: RefCell<Option<Result<(), TeardownError>>>,
    calls: RefCell<Vec<(u64, PathBuf, String)>>,
}

#[cfg(test)]
impl MockTeardown {
    pub(crate) fn ok() -> Self {
        Self {
            canned: RefCell::new(Some(Ok(()))),
            calls: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn failing(err: TeardownError) -> Self {
        Self {
            canned: RefCell::new(Some(Err(err))),
            calls: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn call_count(&self) -> usize {
        self.calls.borrow().len()
    }

    pub(crate) fn last_call(&self) -> Option<(u64, PathBuf, String)> {
        self.calls.borrow().last().cloned()
    }
}

#[cfg(test)]
impl WorktreeTeardownPort for MockTeardown {
    fn wipe(
        &self,
        issue_number: u64,
        path: &Path,
        branch: &str,
        _worktree_root: &Path,
    ) -> Result<(), TeardownError> {
        self.calls
            .borrow_mut()
            .push((issue_number, path.to_path_buf(), branch.to_string()));
        self.canned.borrow_mut().take().unwrap_or(Ok(()))
    }
}

/// Advanceable fake clock. Clones share the same offset cell so a test can
/// advance time after the clock is boxed into the screen.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct FakeClock {
    base: Instant,
    offset: Rc<Cell<Duration>>,
}

#[cfg(test)]
impl FakeClock {
    pub(crate) fn new() -> Self {
        Self {
            base: Instant::now(),
            offset: Rc::new(Cell::new(Duration::ZERO)),
        }
    }

    pub(crate) fn advance(&self, d: Duration) {
        self.offset.set(self.offset.get() + d);
    }
}

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.base + self.offset.get()
    }
}

/// Test-only field accessors. Sibling test modules (and the cross-tree snapshot
/// tests via public methods) drive the screen; these expose the private
/// terminator state for assertions.
#[cfg(test)]
impl InteractionScreen {
    /// Construct a screen with an injected clock seam. Drives the terminator
    /// flow without git or wall-clock sleeps (#741). Teardown is async (#941):
    /// tests resolve the parked [`TeardownDispatch`] through a `MockTeardown`
    /// and feed the outcome to [`Self::apply_teardown_result`].
    pub(crate) fn with_ports(
        issue_number: u64,
        worktree_path: PathBuf,
        branch: String,
        worktree_root: PathBuf,
        clock: Box<dyn Clock>,
    ) -> Self {
        let mut screen = Self::with_history(Vec::new());
        screen.issue_number = issue_number;
        screen.worktree_path = worktree_path;
        screen.branch = branch;
        screen.worktree_root = worktree_root;
        screen.clock = clock;
        screen
    }

    pub(crate) fn history_for_test(&self) -> &[TurnRecord] {
        &self.history
    }

    pub(crate) fn state_for_test(&self) -> InteractionState {
        self.state
    }

    pub(crate) fn close_reason_for_test(&self) -> Option<CloseReason> {
        self.close_reason.clone()
    }

    pub(crate) fn terminated_at_is_set(&self) -> bool {
        self.terminated_at.is_some()
    }

    pub(crate) fn queued_terminator_is_set(&self) -> bool {
        self.queued_terminator.is_some()
    }

    pub(crate) fn force_state_for_test(&mut self, state: InteractionState) {
        self.state = state;
    }

    pub(crate) fn force_terminated_userquit_for_test(&mut self) {
        self.state = InteractionState::Terminated;
        self.close_reason = Some(CloseReason::UserQuit);
    }
}
