//! Test seams for the terminator flow (#741/#941) — `MockTeardown`,
//! `FakeClock`, and the screen's test-only port accessors. Split from
//! `lifecycle.rs` (400-line guardrail). Re-exported from `lifecycle` so
//! existing `lifecycle::{FakeClock, MockTeardown}` imports keep working.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::InteractionScreen;
use super::super::view_state::CloseReason;
use super::{Clock, WorktreeTeardownPort};
use crate::session::interaction::{TurnRecord, TurnState};
use crate::work::worktree_teardown::TeardownError;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Canned [`WorktreeTeardownPort`] for tests. Records each call and returns a
/// pre-seeded outcome on the first call (`take`-once), then `Ok(())`.
pub(crate) struct MockTeardown {
    canned: RefCell<Option<Result<(), TeardownError>>>,
    calls: RefCell<Vec<(u64, PathBuf, String)>>,
}

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
#[derive(Clone)]
pub(crate) struct FakeClock {
    base: Instant,
    offset: Rc<Cell<Duration>>,
}

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

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.base + self.offset.get()
    }
}

/// Test-only field accessors. Sibling test modules (and the cross-tree snapshot
/// tests via public methods) drive the screen; these expose the private
/// terminator state for assertions.
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
        &self.view.turns
    }

    /// History length — test seam for dispatch re-entry assertions (#738).
    pub(crate) fn history_len(&self) -> usize {
        self.view.turns.len()
    }

    /// Open-reviewer flag for snapshot tests / mouse routing (#918).
    pub(crate) fn diff_review_open(&self) -> bool {
        self.diff_review.is_some()
    }

    pub(crate) fn scroll_up_for_test(&mut self, n: usize) {
        self.scroll_up(n);
    }

    pub(crate) fn scroll_down_for_test(&mut self, n: usize) {
        self.scroll_down(n);
    }

    /// Tail-follow flag — cross-module test seam for the mouse-routing
    /// assertion in `tui::mod` (#988).
    pub(crate) fn auto_scroll_for_test(&self) -> bool {
        self.auto_scroll
    }

    /// Terminal-lifecycle flag for the quit-teardown assertions (#950 replaces
    /// the old `state == Terminated` check).
    pub(crate) fn terminated_for_test(&self) -> bool {
        self.terminated
    }

    pub(crate) fn close_reason_for_test(&self) -> Option<CloseReason> {
        self.close_reason.clone()
    }

    pub(crate) fn terminated_at_is_set(&self) -> bool {
        self.terminated_at.is_some()
    }

    /// Screen bound to a synthetic unified interactive session (#948) —
    /// the test twin of the `for_managed` launch path. `turn_state` seeds the
    /// injected view's Idle/Streaming lock (#950).
    pub(crate) fn test_fixture(
        issue: u64,
        produce_pr: bool,
        turn_state: TurnState,
        history: Vec<TurnRecord>,
        worktree: &str,
    ) -> Self {
        let mut session = crate::session::types::Session::new(
            String::new(),
            "opus".to_string(),
            "orchestrator".to_string(),
            Some(issue),
            None,
        );
        session.session_mode = crate::session::types::SessionMode::Interactive;
        session.produce_pr = produce_pr;
        session.turns = history;
        session.turn_state = turn_state;
        let managed = crate::session::manager::ManagedSession::with_worktree(
            session,
            Some(PathBuf::from(worktree)),
            Some(format!("feat/issue-{issue}")),
            None,
        );
        Self::for_managed(&managed)
    }

    pub(crate) fn force_terminated_userquit_for_test(&mut self) {
        self.terminated = true;
        self.close_reason = Some(CloseReason::UserQuit);
    }
}
