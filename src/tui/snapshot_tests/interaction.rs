//! Snapshot tests for the Interaction screen (#736).
//!
//! Covers the four AC render states: empty, one-turn, three-turn mixed
//! roles (incl. a streaming turn), and a scrolled-up transcript.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::session::interaction::{
    CloseReason, InteractionSession, InteractionState, TurnRecord, TurnRole,
};
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event_with_modifiers;
use crate::tui::screens::{InteractionScreen, Screen};
use crate::tui::theme::Theme;
use chrono::{DateTime, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;

const W: u16 = 120;
const H: u16 = 40;

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
    assert_snapshot!(terminal.backend());
}

#[test]
fn interaction_screen_one_turn_user() {
    let mut screen =
        InteractionScreen::with_history(vec![turn(TurnRole::User, "Add a login button", false)]);
    let terminal = render(&mut screen);
    assert_snapshot!(terminal.backend());
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
    assert_snapshot!(terminal.backend());
}

#[test]
fn interaction_screen_scrolled_up() {
    let history: Vec<TurnRecord> = (0..20)
        .map(|i| turn(TurnRole::Agent, &format!("Message {i}"), false))
        .collect();
    let mut screen = InteractionScreen::with_history(history);
    screen.scroll_up_for_test(5);
    let terminal = render(&mut screen);
    assert_snapshot!(terminal.backend());
}

// --- #738: keymap-state render fixtures ---

fn session(
    issue: u64,
    produce_pr: bool,
    state: InteractionState,
    history: Vec<TurnRecord>,
) -> InteractionSession {
    let mut s = InteractionSession::new(
        issue,
        PathBuf::from("/tmp/maestro/issue-42"),
        format!("feat/issue-{issue}"),
        produce_pr,
    );
    s.state = state;
    s.history = history;
    s
}

#[test]
fn interaction_screen_streaming_locks_input() {
    let history = vec![
        turn(TurnRole::User, "implement login", false),
        turn(TurnRole::Agent, "Working on it", true),
    ];
    let mut screen =
        InteractionScreen::for_session(&session(42, true, InteractionState::Streaming, history));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("locked"),
        "streaming must show a locked input pane:\n{rendered}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn interaction_screen_terminated_banner() {
    let history = vec![turn(TurnRole::User, "stop", false)];
    let mut screen = InteractionScreen::for_session(&{
        let mut s = session(42, true, InteractionState::Terminated, history);
        s.close_reason = Some(CloseReason::UserQuit);
        s
    });
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("terminated"),
        "terminated state must show a banner:\n{rendered}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn interaction_screen_quit_modal_open() {
    let mut screen =
        InteractionScreen::for_session(&session(42, true, InteractionState::Idle, vec![]));
    screen.handle_input(
        &key_event_with_modifiers(KeyCode::Char('q'), KeyModifiers::CONTROL),
        InputMode::Insert,
    );
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("Quit interaction?"),
        "quit modal must show the confirm prompt:\n{rendered}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn interaction_screen_ctrl_p_greyed_without_produce_pr() {
    let mut screen =
        InteractionScreen::for_session(&session(42, false, InteractionState::Idle, vec![]));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("off"),
        "Ctrl+P must render greyed/off when produce_pr is false:\n{rendered}"
    );
    assert_snapshot!(terminal.backend());
}
