//! Unit tests for the Interaction screen keymap, quit modal, and turn
//! event application (#738). Split from `tests.rs` for the file-size budget.

use super::*;
use crate::session::interaction::{TurnEvent, TurnRecord, TurnRole, TurnState};
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
use chrono::{DateTime, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};

fn fixed_t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap()
}

fn user_turn(content: &str) -> TurnRecord {
    TurnRecord {
        role: TurnRole::User,
        content: content.into(),
        started_at: fixed_t0(),
        finished_at: Some(fixed_t0()),
    }
}

fn three_turn_fixture() -> Vec<TurnRecord> {
    vec![
        user_turn("one"),
        TurnRecord {
            role: TurnRole::Agent,
            content: "two".into(),
            started_at: fixed_t0(),
            finished_at: Some(fixed_t0()),
        },
        TurnRecord {
            role: TurnRole::System,
            content: "three".into(),
            started_at: fixed_t0(),
            finished_at: Some(fixed_t0()),
        },
    ]
}

fn screen_for(issue: u64, produce_pr: bool, turn_state: TurnState) -> InteractionScreen {
    InteractionScreen::test_fixture(
        issue,
        produce_pr,
        turn_state,
        Vec::new(),
        "/tmp/maestro/wt-x",
    )
}

/// Idle screen forced into the terminal lifecycle (#950: terminated is a
/// screen-local bool, no longer a `turn_state` value).
fn terminated_screen(issue: u64, produce_pr: bool) -> InteractionScreen {
    let mut s = screen_for(issue, produce_pr, TurnState::Idle);
    s.force_terminated_userquit_for_test();
    s
}

fn type_text(s: &mut InteractionScreen, text: &str) {
    for c in text.chars() {
        s.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
    }
}

fn ctrl(c: char) -> crossterm::event::Event {
    key_event_with_modifiers(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn enter_idle_nonempty_locks_editor_and_sends_turn() {
    let mut s = screen_for(7, true, TurnState::Idle);
    type_text(&mut s, "fix it");
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(
        action,
        ScreenAction::SendInteractionTurn {
            issue_number: 7,
            prompt: "fix it".to_string(),
        }
    );
    assert_eq!(s.editor_text(), "");
    // #950: the streaming lock now comes from the live session via the view;
    // begin_turn only returns the send action.
    assert!(!s.is_streaming());
}

#[test]
fn enter_idle_empty_editor_is_noop() {
    let mut s = screen_for(7, true, TurnState::Idle);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(!s.is_streaming());
}

#[test]
fn shift_enter_inserts_newline_does_not_send() {
    let mut s = screen_for(7, true, TurnState::Idle);
    type_text(&mut s, "line1");
    let action = s.handle_input(
        &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
        InputMode::Insert,
    );
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.editor_text(), "line1\n");
    assert!(!s.is_streaming());
}

#[test]
fn ctrl_p_produce_pr_true_sends_pushup_prompt() {
    let mut s = screen_for(42, true, TurnState::Idle);
    let action = s.handle_input(&ctrl('p'), InputMode::Insert);
    match action {
        ScreenAction::SendInteractionTurn {
            issue_number,
            prompt,
        } => {
            assert_eq!(issue_number, 42);
            assert!(prompt.contains("/pushup"), "got: {prompt}");
            assert!(prompt.contains("#42"), "got: {prompt}");
        }
        other => panic!("expected SendInteractionTurn, got {other:?}"),
    }
}

#[test]
fn ctrl_p_produce_pr_false_logs_and_does_not_send() {
    let mut s = screen_for(42, false, TurnState::Idle);
    let action = s.handle_input(&ctrl('p'), InputMode::Insert);
    match action {
        ScreenAction::LogActivity { tag, message, .. } => {
            assert_eq!(tag, "INTERACTION");
            assert!(message.contains("disabled"), "got: {message}");
        }
        other => panic!("expected LogActivity, got {other:?}"),
    }
    assert!(!s.is_streaming());
}

#[test]
fn ctrl_l_clears_editor_leaves_history() {
    let mut s = InteractionScreen::test_fixture(
        7,
        true,
        TurnState::Idle,
        three_turn_fixture(),
        "/tmp/maestro/wt-x",
    );
    type_text(&mut s, "partial");
    let action = s.handle_input(&ctrl('l'), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.editor_text(), "");
    assert_eq!(s.history_len(), 3);
}

#[test]
fn esc_idle_returns_pop() {
    let mut s = screen_for(7, true, TurnState::Idle);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::Pop);
    assert!(!s.quit_modal_open);
}

#[test]
fn esc_streaming_returns_pop() {
    let mut s = screen_for(7, true, TurnState::Streaming);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn streaming_ignores_enter() {
    let mut s = screen_for(7, true, TurnState::Streaming);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(s.is_streaming());
}

#[test]
fn streaming_ignores_ctrl_l() {
    let mut s = screen_for(7, true, TurnState::Streaming);
    let action = s.handle_input(&ctrl('l'), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn streaming_scroll_up_still_works() {
    let mut s = InteractionScreen::test_fixture(
        7,
        true,
        TurnState::Streaming,
        three_turn_fixture(),
        "/tmp/maestro/wt-x",
    );
    s.scroll_offset = 5;
    s.auto_scroll = false;
    s.handle_input(&key_event(KeyCode::Up), InputMode::Insert);
    assert_eq!(s.scroll_offset, 4);
}

#[test]
fn terminated_any_key_returns_pop() {
    let mut s = terminated_screen(7, true);
    assert_eq!(
        s.handle_input(&key_event(KeyCode::Char('x')), InputMode::Insert),
        ScreenAction::Pop
    );
    assert_eq!(
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert),
        ScreenAction::Pop
    );
}

// --- quit modal ---

#[test]
fn ctrl_w_opens_quit_modal() {
    let mut s = screen_for(7, true, TurnState::Idle);
    let action = s.handle_input(&ctrl('w'), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(s.quit_modal_open);
}

#[test]
fn ctrl_w_streaming_opens_quit_modal() {
    let mut s = screen_for(7, true, TurnState::Streaming);
    s.handle_input(&ctrl('w'), InputMode::Insert);
    assert!(s.quit_modal_open);
    assert!(s.is_streaming());
}

#[test]
fn quit_modal_y_returns_quit_action_without_terminating() {
    // #949: the app terminates the session + starts the wipe; the screen
    // terminates only when the teardown result lands.
    let mut s = screen_for(9, true, TurnState::Idle);
    s.handle_input(&ctrl('w'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Char('y')), InputMode::Insert);
    assert_eq!(action, ScreenAction::QuitInteraction { issue_number: 9 });
    assert!(!s.terminated_for_test());
    assert_eq!(s.close_reason, None);
    assert!(!s.quit_modal_open);
}

#[test]
fn quit_modal_uppercase_y_returns_quit_action() {
    let mut s = screen_for(9, true, TurnState::Idle);
    s.handle_input(&ctrl('w'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Char('Y')), InputMode::Insert);
    assert_eq!(action, ScreenAction::QuitInteraction { issue_number: 9 });
    assert!(!s.terminated_for_test());
}

#[test]
fn quit_modal_n_cancels() {
    let mut s = screen_for(9, true, TurnState::Idle);
    s.handle_input(&ctrl('w'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Char('n')), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(!s.quit_modal_open);
    assert!(!s.is_streaming());
}

#[test]
fn quit_modal_esc_cancels_modal_not_screen() {
    let mut s = screen_for(9, true, TurnState::Idle);
    s.handle_input(&ctrl('w'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(!s.quit_modal_open);
}

// --- keybindings provider ---

#[test]
fn keybindings_list_expected_keys() {
    let s = screen_for(7, true, TurnState::Idle);
    let keys: Vec<&str> = s
        .keybindings()
        .into_iter()
        .flat_map(|g| g.bindings.into_iter().map(|b| b.key))
        .collect();
    for expected in [
        "Enter",
        "Shift+Enter",
        "Ctrl+P",
        "Ctrl+L",
        "Ctrl+W",
        "Esc",
        "Up/Down/PgUp/PgDn",
        "End",
    ] {
        assert!(keys.contains(&expected), "missing {expected} in {keys:?}");
    }
}

#[test]
fn keybindings_ctrl_p_greyed_when_produce_pr_false() {
    let s = screen_for(7, false, TurnState::Idle);
    let desc = s
        .keybindings()
        .into_iter()
        .flat_map(|g| g.bindings)
        .find(|b| b.key == "Ctrl+P")
        .map(|b| b.description)
        .unwrap_or("");
    assert!(desc.contains("greyed"), "got: {desc}");
}

// --- log_turn_event (#950: telemetry + activity-log only; the transcript
// lives on the live session, written by the pipeline) ---

#[test]
fn log_turn_started_and_chunks_return_none() {
    let mut s = screen_for(7, true, TurnState::Idle);
    let at = fixed_t0();
    assert_eq!(
        s.log_turn_event(&TurnEvent::TurnStarted {
            role: TurnRole::Agent,
            at,
        }),
        ScreenAction::None
    );
    assert_eq!(
        s.log_turn_event(&TurnEvent::Chunk("hello".into())),
        ScreenAction::None
    );
}

#[test]
fn log_turn_finished_emits_chunk_count_and_duration() {
    let mut s = screen_for(13, true, TurnState::Idle);
    // Simulate a send so turn_count == 1.
    type_text(&mut s, "hi");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let start = fixed_t0();
    s.log_turn_event(&TurnEvent::TurnStarted {
        role: TurnRole::Agent,
        at: start,
    });
    s.log_turn_event(&TurnEvent::Chunk("a".into()));
    s.log_turn_event(&TurnEvent::Chunk("b".into()));
    let finished = start + chrono::Duration::milliseconds(250);
    let action = s.log_turn_event(&TurnEvent::TurnFinished { at: finished });
    match action {
        ScreenAction::LogActivity { tag, message, .. } => {
            assert_eq!(tag, "INTERACTION");
            assert!(message.contains("#13"), "got: {message}");
            assert!(message.contains("turn 1"), "got: {message}");
            assert!(message.contains("2 chunks streamed"), "got: {message}");
            assert!(message.contains("250 ms"), "got: {message}");
        }
        other => panic!("expected LogActivity, got {other:?}"),
    }
}

#[test]
fn log_error_returns_turn_failed_activity() {
    let mut s = screen_for(7, true, TurnState::Streaming);
    // #742: a failed turn leaves exactly one log line. The System turn for
    // the error is pushed to the session by the pipeline, not the screen.
    let action = s.log_turn_event(&TurnEvent::Error("agent exit 1".into()));
    let ScreenAction::LogActivity { tag, message, .. } = action else {
        panic!("expected a TurnFailed LogActivity, got {action:?}");
    };
    assert_eq!(tag, "INTERACTION");
    assert_eq!(message, "#7 turn failed: agent exit 1");
}

#[test]
fn set_issue_title_sanitizes_terminal_escapes() {
    // Security (#738): external GitHub titles must not inject terminal escapes
    // via the header/starter-hint renderers.
    let mut s = screen_for(7, true, TurnState::Idle);
    s.set_issue_title("evil\x1b[2Jtitle\u{7}");
    // No ESC or BEL control chars survive (newlines are allowed by the helper).
    assert!(
        !s.issue_title.contains('\x1b'),
        "ESC must be stripped: {:?}",
        s.issue_title
    );
    assert!(
        !s.issue_title.contains('\u{7}'),
        "BEL must be stripped: {:?}",
        s.issue_title
    );
    assert!(s.issue_title.contains("evil"));
    assert!(s.issue_title.contains("title"));
}
