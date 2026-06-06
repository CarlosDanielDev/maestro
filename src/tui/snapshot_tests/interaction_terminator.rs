//! Snapshot tests for the terminator UI flow (#741).
//!
//! Four renders: idle-fire success, streaming-deferred, teardown-failure, and
//! UserQuit-before-terminator. All use `FakeClock` so `terminated_at` is frozen
//! and never trips auto-nav mid-render. Drives the screen only through public
//! seams (this module lives outside the `interaction` privacy boundary).
//!
//! Turn cards render a `role · HH:MM` header (#987) where the time is the
//! turn's wall-clock `started_at`. Those turns are stamped by production code
//! with `Utc::now()`, so the rendered time is non-deterministic — every
//! snapshot assertion here masks it via [`with_time_mask`].

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::session::interaction_lifecycle::InteractionLifecycleEvent;
use crate::tui::navigation::InputMode;
use crate::tui::screens::interaction::lifecycle::{FakeClock, MockTeardown};
use crate::tui::screens::test_helpers::key_event_with_modifiers;
use crate::tui::screens::{InteractionScreen, Screen};
use crate::tui::theme::Theme;
use crate::work::worktree_teardown::TeardownError;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;
use std::rc::Rc;

const W: u16 = 120;
const H: u16 = 40;

fn pr_event(pr_number: u64) -> InteractionLifecycleEvent {
    InteractionLifecycleEvent::PrLinkedToIssue {
        pr_number,
        issue_number: 42,
        owner: "owner".into(),
        repo: "repo".into(),
    }
}

fn base_screen(teardown: MockTeardown) -> InteractionScreen {
    InteractionScreen::with_ports(
        42,
        PathBuf::from("/tmp/maestro/issue-42"),
        "feat/issue-42".to_string(),
        PathBuf::from("/tmp/maestro"),
        Box::new(Rc::new(teardown)),
        Box::new(FakeClock::new()),
    )
}

fn render(screen: &mut InteractionScreen) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    let theme = Theme::dark();
    terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
    terminal
}

/// Run `body` with the `· HH:MM ` card-header time masked to a fixed token, so
/// the wall-clock `started_at` on production-stamped turns can't flake the
/// snapshot. The replacement is the same width (5 cols) as `HH:MM`, so border
/// alignment is preserved.
fn with_time_mask(body: impl FnOnce()) {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"· \d{2}:\d{2} ", "· HH:MM ");
    settings.bind(body);
}

#[test]
fn terminator_idle_success() {
    let mut screen = base_screen(MockTeardown::ok());
    screen.on_terminator_signaled(pr_event(7));

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("PR #7"),
        "must show the PR turn:\n{rendered}"
    );
    assert!(
        rendered.contains("terminated"),
        "must show the terminated banner:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn terminator_streaming_deferred() {
    let mut screen = base_screen(MockTeardown::ok());
    // Public seam: seed_turn pushes a User turn and flips to Streaming.
    screen.seed_turn("implement login".to_string());
    screen.on_terminator_signaled(pr_event(7));

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("locked"),
        "deferred state stays streaming with a locked input:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn terminator_teardown_failure() {
    let err = TeardownError::PathStillExists(PathBuf::from("/tmp/maestro/issue-42"));
    let mut screen = base_screen(MockTeardown::failing(err));
    screen.on_terminator_signaled(pr_event(7));

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("teardown failed"),
        "must surface the failure turn:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn terminator_userquit_before_terminator() {
    let mut screen = base_screen(MockTeardown::ok());
    // Public path to Terminated(UserQuit): Ctrl+W then confirm 'y'.
    screen.handle_input(
        &key_event_with_modifiers(KeyCode::Char('w'), KeyModifiers::CONTROL),
        InputMode::Insert,
    );
    screen.handle_input(
        &key_event_with_modifiers(KeyCode::Char('y'), KeyModifiers::NONE),
        InputMode::Insert,
    );
    // A later marker must NOT run teardown nor append turns.
    screen.on_terminator_signaled(pr_event(7));

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("terminated"),
        "must show the terminated banner:\n{rendered}"
    );
    // No card header is rendered in the UserQuit-before-terminator state (the
    // marker is ignored, no turn appended), so there is no `· HH:MM` to mask.
    assert_snapshot!(terminal.backend());
}
