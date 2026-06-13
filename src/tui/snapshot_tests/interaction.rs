//! Snapshot tests for the Interaction screen (#736).
//!
//! Covers the four AC render states: empty, one-turn, three-turn mixed
//! roles (incl. a streaming turn), and a scrolled-up transcript.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::session::interaction::{TurnRecord, TurnRole, TurnState};
use crate::session::types::SessionStatus;
use crate::tui::navigation::InputMode;
use crate::tui::screens::InteractionView;
use crate::tui::screens::test_helpers::key_event_with_modifiers;
use crate::tui::screens::{InteractionScreen, Screen};
use crate::tui::theme::Theme;
use chrono::{DateTime, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};

const W: u16 = 120;
const H: u16 = 40;

/// Run `body` with the `· HH:MM ` card-header time masked to a fixed token.
/// The header renders in the machine's local zone (#987 QA), so the exact
/// value is non-deterministic across machines/CI — mask it for portability.
/// Same width (5 cols) as `HH:MM`, so border alignment is preserved.
fn with_time_mask(body: impl FnOnce()) {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"· \d{2}:\d{2} ", "· HH:MM ");
    settings.bind(body);
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap()
}

fn t1() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 30).unwrap()
}

fn turn(role: TurnRole, content: &str, streaming: bool) -> TurnRecord {
    TurnRecord {
        role,
        content: content.to_string(),
        started_at: t0(),
        finished_at: if streaming { None } else { Some(t1()) },
    }
}

fn render(screen: &mut InteractionScreen) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    let theme = Theme::dark();
    terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
    terminal
}

#[test]
fn interaction_screen_empty_state() {
    let mut screen = InteractionScreen::new();
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("No messages"),
        "empty state must show an action-oriented message:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_one_turn_user() {
    let mut screen =
        InteractionScreen::with_history(vec![turn(TurnRole::User, "Add a login button", false)]);
    let terminal = render(&mut screen);
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_three_turns_mixed_with_streaming() {
    let mut screen = InteractionScreen::with_history(vec![
        turn(TurnRole::System, "Session started for #736", false),
        turn(TurnRole::User, "Add a login button", false),
        turn(TurnRole::Agent, "Done. Added LoginButton.", true),
    ]);
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains('…'),
        "streaming turn must show a trailing indicator:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_scrolled_up() {
    let history: Vec<TurnRecord> = (0..20)
        .map(|i| turn(TurnRole::Agent, &format!("Message {i}"), false))
        .collect();
    let mut screen = InteractionScreen::with_history(history);
    screen.scroll_up_for_test(5);
    let terminal = render(&mut screen);
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

// --- #738: keymap-state render fixtures ---

fn screen(
    issue: u64,
    produce_pr: bool,
    turn_state: TurnState,
    history: Vec<TurnRecord>,
) -> InteractionScreen {
    InteractionScreen::test_fixture(
        issue,
        produce_pr,
        turn_state,
        history,
        "/tmp/maestro/issue-42",
    )
}

/// Build the view a settled session would inject (#950).
fn settled_view(
    turns: Vec<TurnRecord>,
    settled_from: SessionStatus,
    pr_linked: Option<u64>,
) -> InteractionView {
    InteractionView {
        turns,
        turn_state: TurnState::Idle,
        settled_from: Some(settled_from),
        pr_linked,
    }
}

#[test]
fn interaction_screen_streaming_locks_input() {
    let history = vec![
        turn(TurnRole::User, "implement login", false),
        turn(TurnRole::Agent, "Working on it", true),
    ];
    let mut screen = screen(42, true, TurnState::Streaming, history);
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("locked"),
        "streaming must show a locked input pane:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_terminated_banner() {
    let history = vec![turn(TurnRole::User, "stop", false)];
    let mut screen = screen(42, true, TurnState::Idle, history);
    screen.force_terminated_userquit_for_test();
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("terminated"),
        "terminated state must show a banner:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_settled_completed_banner() {
    let mut screen = screen(42, true, TurnState::Idle, vec![]);
    screen.set_view(settled_view(
        vec![turn(TurnRole::User, "ship it", false)],
        SessionStatus::Completed,
        None,
    ));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("COMPLETED"),
        "settled banner must name the outcome:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_settled_with_pr_banner() {
    let mut screen = screen(42, true, TurnState::Idle, vec![]);
    screen.set_view(settled_view(
        vec![turn(TurnRole::System, "PR opened", false)],
        SessionStatus::Completed,
        Some(7),
    ));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("PR #7"),
        "banner must show the PR:\n{rendered}"
    );
    assert!(
        rendered.contains("COMPLETED"),
        "banner must show outcome:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_settled_failed_gates_banner() {
    let mut screen = screen(42, true, TurnState::Idle, vec![]);
    screen.set_view(settled_view(
        vec![turn(TurnRole::System, "gates failed", false)],
        SessionStatus::FailedGates,
        None,
    ));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.to_lowercase().contains("retry") || rendered.contains("FAILED_GATES"),
        "failed-gates banner must invite a retry:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_quit_modal_open() {
    let mut screen = screen(42, true, TurnState::Idle, vec![]);
    screen.handle_input(
        &key_event_with_modifiers(KeyCode::Char('w'), KeyModifiers::CONTROL),
        InputMode::Insert,
    );
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("Quit interaction?"),
        "quit modal must show the confirm prompt:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_ctrl_p_greyed_without_produce_pr() {
    let mut screen = screen(42, false, TurnState::Idle, vec![]);
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("off"),
        "Ctrl+P must render greyed/off when produce_pr is false:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_screen_header_shows_agent_and_model() {
    let mut screen = screen(7, true, TurnState::Idle, vec![]);
    screen.set_provider_context("claude", "opus");
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("claude"),
        "header must name the agent:\n{rendered}"
    );
    assert!(
        rendered.contains("opus"),
        "header must name the model:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}
