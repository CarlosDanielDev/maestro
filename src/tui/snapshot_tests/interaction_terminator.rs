//! Snapshot tests for the PR-linked + quit-teardown UI flow (#741 → #949).
//!
//! Five renders: PR-linked keeps the chat open, quit success, the #941
//! async in-flight state, quit teardown failure, and the untrusted-root
//! quit skip. All use `FakeClock` so `terminated_at` is frozen and never
//! trips auto-nav mid-render. Teardown resolves asynchronously (#941):
//! tests park the dispatch, resolve it through `MockTeardown`, and apply
//! the result — the same dance the app's spawn_blocking dispatcher
//! performs.
//!
//! Turn cards render a `role · HH:MM` header (#987) where the time is the
//! turn's wall-clock `started_at`. Those turns are stamped by production code
//! with `Utc::now()`, so the rendered time is non-deterministic — every
//! snapshot assertion here masks it via [`with_time_mask`].

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::session::interaction::{TurnRecord, TurnRole, TurnState};
use crate::tui::screens::interaction::lifecycle::{FakeClock, MockTeardown, WorktreeTeardownPort};
use crate::tui::screens::{InteractionScreen, InteractionView, Screen};
use crate::tui::theme::Theme;
use crate::work::worktree_teardown::TeardownError;
use std::path::PathBuf;

const W: u16 = 120;
const H: u16 = 40;

fn base_screen() -> InteractionScreen {
    InteractionScreen::with_ports(
        42,
        PathBuf::from("/tmp/maestro/issue-42"),
        "feat/issue-42".to_string(),
        PathBuf::from("/tmp/maestro"),
        Box::new(FakeClock::new()),
    )
}

/// Resolve a parked teardown dispatch through `teardown` (#941), mirroring
/// the app's spawn_blocking dispatcher.
fn resolve_teardown(screen: &mut InteractionScreen, teardown: &MockTeardown) {
    if let Some(d) = screen.take_pending_teardown_dispatch() {
        let result = teardown
            .wipe(d.issue_number, &d.path, &d.branch, &d.root)
            .map_err(|err| err.to_string());
        let _ = screen.apply_teardown_result(result);
    }
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
fn pr_linked_keeps_chat_open() {
    // #949 (spec §4.4): a linked PR posts a System turn and the chat stays
    // open — editable input, no banner, no teardown. #950: that System turn
    // is written to the live session by the pipeline and rendered through the
    // injected view, so the test seeds the view the pipeline would produce.
    let mut screen = base_screen();
    let _ = screen.on_pr_linked(7);
    let now = chrono::Utc::now();
    screen.set_view(InteractionView {
        turns: vec![TurnRecord {
            role: TurnRole::System,
            content: "PR #7 created — session stays open (Ctrl+W to quit)".to_string(),
            started_at: now,
            finished_at: Some(now),
        }],
        turn_state: TurnState::Idle,
        settled_from: None,
        pr_linked: Some(7),
    });

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("PR #7"),
        "must show the PR announcement turn:\n{rendered}"
    );
    assert!(
        rendered.contains("session stays open"),
        "the announcement names the kept-alive behavior:\n{rendered}"
    );
    assert!(
        !rendered.contains("Terminated"),
        "no terminated banner on PR detection:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn quit_teardown_success() {
    let teardown = MockTeardown::ok();
    let mut screen = base_screen();
    let _ = screen.begin_quit_teardown();
    resolve_teardown(&mut screen, &teardown);

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("worktree removed"),
        "must show the teardown result turn:\n{rendered}"
    );
    assert!(
        rendered.contains("terminated"),
        "must show the terminated banner:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn quit_teardown_in_flight() {
    // #941: between dispatch and result the input pane is replaced by the
    // "wiping worktree" banner — the UI is alive, not frozen.
    let mut screen = base_screen();
    let _ = screen.begin_quit_teardown();

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("wiping worktree"),
        "must show the in-flight banner:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn quit_teardown_failure() {
    let err = TeardownError::PathStillExists(PathBuf::from("/tmp/maestro/issue-42"));
    let teardown = MockTeardown::failing(err);
    let mut screen = base_screen();
    let _ = screen.begin_quit_teardown();
    resolve_teardown(&mut screen, &teardown);

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("teardown failed"),
        "must surface the failure turn:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn quit_skip_without_trusted_root() {
    // cwd-fallback: no isolated worktree → quit terminates inline, no wipe.
    let mut screen = InteractionScreen::with_ports(
        42,
        PathBuf::from("."),
        "maestro/issue-42".to_string(),
        PathBuf::new(), // empty root
        Box::new(FakeClock::new()),
    );
    let _ = screen.begin_quit_teardown();

    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("no isolated worktree"),
        "must show the skip turn:\n{rendered}"
    );
    assert!(
        rendered.contains("terminated"),
        "must show the terminated banner:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}
