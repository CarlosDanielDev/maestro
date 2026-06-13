//! Unit tests for the quit-teardown + PR-linked lifecycle on
//! `InteractionScreen` (#741 → #949).
//!
//! Covers the #949 AC behaviors — PR-linked keeps the session open (no
//! wipe, no state change), quit wipes once via the port, teardown failure
//! surfaces the error, the untrusted-root skip path — plus the 500ms
//! auto-nav timer and the #941 async split (dispatch parked → in-flight
//! banner state → result applied). Uses `MockTeardown` + `FakeClock` (from
//! `lifecycle`) so no git or disk is touched; tests resolve the parked
//! dispatch through the mock exactly as the app's spawn_blocking
//! dispatcher does.

use super::InteractionScreen;
use super::lifecycle::{FakeClock, MockTeardown, WorktreeTeardownPort};
use super::view_state::{CloseReason, InteractionState};
use crate::session::interaction::{TurnRecord, TurnRole};
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::ScreenAction;
use crate::work::worktree_teardown::TeardownError;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Screen wired with a boxed clone of `teardown` + `clock` for issue 42,
/// worktree `/tmp/maestro/issue-42` (root `/tmp/maestro`), branch
/// `feat/issue-42`. Returns the screen; the caller keeps the `Rc`/clock
/// handles for post-call inspection.
fn wired_screen(_teardown: Rc<MockTeardown>, clock: FakeClock) -> InteractionScreen {
    InteractionScreen::with_ports(
        42,
        PathBuf::from("/tmp/maestro/issue-42"),
        "feat/issue-42".to_string(),
        PathBuf::from("/tmp/maestro"),
        Box::new(clock),
    )
}

/// Resolve a parked teardown dispatch through `teardown`, mirroring the app's
/// `spawn_blocking` dispatcher (#941). Returns the result-side action, or
/// `None` when nothing was dispatched.
fn resolve_teardown(
    screen: &mut InteractionScreen,
    teardown: &MockTeardown,
) -> Option<crate::tui::screens::ScreenAction> {
    let d = screen.take_pending_teardown_dispatch()?;
    let result = teardown
        .wipe(d.issue_number, &d.path, &d.branch, &d.root)
        .map_err(|err| err.to_string());
    Some(screen.apply_teardown_result(result))
}

fn system_turns(screen: &InteractionScreen) -> Vec<&TurnRecord> {
    screen
        .history_for_test()
        .iter()
        .filter(|t| t.role == TurnRole::System)
        .collect()
}

// ── #949 AC-1: PR keeps the session alive ────────────────────────────────────

#[test]
fn pr_linked_keeps_session_open_without_teardown() {
    let teardown = Rc::new(MockTeardown::ok());
    let mut screen = wired_screen(Rc::clone(&teardown), FakeClock::new());

    let action = screen.on_pr_linked(7);

    assert_eq!(
        screen.state_for_test(),
        InteractionState::Idle,
        "a linked PR must NOT change the screen state (#949, spec §4.4)"
    );
    assert_eq!(screen.close_reason_for_test(), None);
    assert!(!screen.is_teardown_in_flight());
    assert!(
        screen.take_pending_teardown_dispatch().is_none(),
        "PR detection must never park a destructive dispatch"
    );
    assert_eq!(teardown.call_count(), 0, "no wipe on PR detection");
    assert!(
        !screen.poll_auto_nav(),
        "no auto-navigation on PR detection"
    );

    let sys = system_turns(&screen);
    assert_eq!(sys.len(), 1, "one System announcement");
    assert!(sys[0].content.contains("PR #7"));
    assert!(sys[0].content.contains("session stays open"));
    match action {
        ScreenAction::LogActivity { message, .. } => {
            assert!(
                message.contains("PR #7") && message.contains("stays open"),
                "got: {message:?}"
            );
        }
        other => panic!("expected a LogActivity, got {other:?}"),
    }
}

// ── #949 AC-2: quit wipes once via the port ──────────────────────────────────

#[test]
fn quit_teardown_wipes_once_and_terminates_userquit() {
    let teardown = Rc::new(MockTeardown::ok());
    let mut screen = wired_screen(Rc::clone(&teardown), FakeClock::new());

    let begin_action = screen.begin_quit_teardown();

    // #941: between dispatch and result the screen is in-flight, not frozen.
    assert!(
        matches!(begin_action, ScreenAction::None),
        "the TEARDOWN log line arrives with the async result"
    );
    assert_ne!(screen.state_for_test(), InteractionState::Terminated);
    assert!(screen.is_teardown_in_flight());
    assert_eq!(teardown.call_count(), 0, "no blocking wipe on the UI path");

    let action = resolve_teardown(&mut screen, &teardown);

    assert_eq!(screen.state_for_test(), InteractionState::Terminated);
    assert!(!screen.is_teardown_in_flight());
    assert_eq!(screen.close_reason_for_test(), Some(CloseReason::UserQuit));
    assert_eq!(teardown.call_count(), 1, "teardown must be called once");
    assert!(screen.terminated_at_is_set());
    assert!(
        matches!(action, Some(ScreenAction::LogActivity { .. })),
        "the result must produce a TEARDOWN activity-log action"
    );

    let sys = system_turns(&screen);
    assert_eq!(sys.len(), 2, "two System turns appended on success");
    assert!(sys[0].content.contains("quitting"));
    assert!(sys[0].content.contains("wiping worktree"));
    assert!(
        sys[1]
            .content
            .contains("worktree removed at /tmp/maestro/issue-42")
    );
    assert!(sys[1].content.contains("branch feat/issue-42 deleted"));
}

#[test]
fn quit_teardown_receives_issue_path_branch() {
    let teardown = Rc::new(MockTeardown::ok());
    let mut screen = wired_screen(Rc::clone(&teardown), FakeClock::new());

    screen.begin_quit_teardown();
    resolve_teardown(&mut screen, &teardown);

    let call = teardown.last_call().expect("one call recorded");
    assert_eq!(call.0, 42);
    assert_eq!(call.1, PathBuf::from("/tmp/maestro/issue-42"));
    assert_eq!(call.2, "feat/issue-42");
}

#[test]
fn quit_skips_teardown_when_no_trusted_worktree_root() {
    // cwd-fallback: worktree_root is empty → never run the destructive wipe.
    let teardown = Rc::new(MockTeardown::ok());
    let mut screen = InteractionScreen::with_ports(
        42,
        PathBuf::from("."),
        "maestro/issue-42".to_string(),
        PathBuf::new(), // empty root
        Box::new(FakeClock::new()),
    );

    let action = screen.begin_quit_teardown();

    assert_eq!(teardown.call_count(), 0, "no wipe without a trusted root");
    assert!(
        screen.take_pending_teardown_dispatch().is_none(),
        "the skip path must not park a destructive dispatch"
    );
    assert_eq!(screen.state_for_test(), InteractionState::Terminated);
    assert_eq!(screen.close_reason_for_test(), Some(CloseReason::UserQuit));
    let sys = system_turns(&screen);
    assert!(
        sys.last()
            .unwrap()
            .content
            .contains("no isolated worktree to remove")
    );
    assert!(matches!(action, ScreenAction::LogActivity { .. }));
}

// ── #949 AC-3: teardown failure ──────────────────────────────────────────────

#[test]
fn quit_teardown_failure_keeps_worktree_and_surfaces_error() {
    let err = TeardownError::PathStillExists(PathBuf::from("/tmp/maestro/issue-42"));
    let err_string = err.to_string();
    let teardown = Rc::new(MockTeardown::failing(err));
    let mut screen = wired_screen(Rc::clone(&teardown), FakeClock::new());

    screen.begin_quit_teardown();
    let action = resolve_teardown(&mut screen, &teardown);

    assert_eq!(screen.state_for_test(), InteractionState::Terminated);
    assert_eq!(
        screen.close_reason_for_test(),
        Some(CloseReason::AgentFailure {
            tail: err_string.clone()
        })
    );
    assert!(matches!(
        action,
        Some(ScreenAction::LogActivity {
            level: LogLevel::Warn,
            ..
        })
    ));

    let sys = system_turns(&screen);
    let last = sys.last().expect("a failure System turn");
    assert!(last.content.contains("worktree teardown failed"));
    assert!(last.content.contains(&err_string));
    assert!(
        last.content
            .contains("manual cleanup: git worktree remove /tmp/maestro/issue-42")
    );
}

#[test]
fn stale_teardown_result_is_ignored() {
    let teardown = Rc::new(MockTeardown::ok());
    let mut screen = wired_screen(Rc::clone(&teardown), FakeClock::new());

    let action = screen.apply_teardown_result(Ok(()));

    assert!(matches!(action, ScreenAction::None));
    assert_ne!(
        screen.state_for_test(),
        InteractionState::Terminated,
        "a stale result must not terminate the screen"
    );
}

// ── #949 AC-5: 500ms auto-nav timer after quit ───────────────────────────────

#[test]
fn poll_auto_nav_false_before_delay_true_at_delay() {
    let teardown = Rc::new(MockTeardown::ok());
    let clock = FakeClock::new();
    let mut screen = wired_screen(Rc::clone(&teardown), clock.clone());

    screen.begin_quit_teardown();
    resolve_teardown(&mut screen, &teardown);
    assert!(screen.terminated_at_is_set());

    assert!(!screen.poll_auto_nav(), "no time elapsed");
    clock.advance(Duration::from_millis(499));
    assert!(!screen.poll_auto_nav(), "499ms < 500ms");
    clock.advance(Duration::from_millis(1));
    assert!(screen.poll_auto_nav(), "500ms reached");
}

#[test]
fn poll_auto_nav_false_when_not_terminated() {
    let teardown = Rc::new(MockTeardown::ok());
    let screen = wired_screen(Rc::clone(&teardown), FakeClock::new());
    assert!(!screen.poll_auto_nav(), "Idle screen never auto-navigates");
}
