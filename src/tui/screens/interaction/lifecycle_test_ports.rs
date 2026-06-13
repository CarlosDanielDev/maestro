//! Test seams for the terminator flow (#741/#941) — `MockTeardown`,
//! `FakeClock`, and the screen's test-only port accessors. Split from
//! `lifecycle.rs` (400-line guardrail). Re-exported from `lifecycle` so
//! existing `lifecycle::{FakeClock, MockTeardown}` imports keep working.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::InteractionScreen;
use super::super::view_state::{CloseReason, InteractionState};
use super::{Clock, WorktreeTeardownPort};
use crate::session::interaction::TurnRecord;
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

    pub(crate) fn force_state_for_test(&mut self, state: InteractionState) {
        self.state = state;
    }

    /// Screen bound to a synthetic unified interactive session (#948) —
    /// the test twin of the `for_managed` launch path.
    pub(crate) fn test_fixture(
        issue: u64,
        produce_pr: bool,
        state: InteractionState,
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
        let managed = crate::session::manager::ManagedSession::with_worktree(
            session,
            Some(PathBuf::from(worktree)),
            Some(format!("feat/issue-{issue}")),
            None,
        );
        let mut screen = Self::for_managed(&managed);
        screen.force_state_for_test(state);
        screen
    }

    pub(crate) fn force_terminated_userquit_for_test(&mut self) {
        self.state = InteractionState::Terminated;
        self.close_reason = Some(CloseReason::UserQuit);
    }
}
