//! Unit tests for the Interaction screen keymap, quit modal, and turn
//! event application (#738). Split from `tests.rs` for the file-size budget.

use super::*;
use crate::session::interaction::{
    CloseReason, InteractionSession, InteractionState, TurnRecord, TurnRole,
};
use crate::session::interaction_turn::TurnEvent;
use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
use chrono::{DateTime, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;

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

fn session_for(issue: u64, produce_pr: bool, state: InteractionState) -> InteractionSession {
    let mut s = InteractionSession::new(
        issue,
        PathBuf::from("/tmp/maestro/wt-x"),
        format!("feat/issue-{issue}"),
        produce_pr,
    );
    s.state = state;
    s
}

fn screen_for(issue: u64, produce_pr: bool, state: InteractionState) -> InteractionScreen {
    InteractionScreen::for_session(&session_for(issue, produce_pr, state))
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
    let mut s = screen_for(7, true, InteractionState::Idle);
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
    assert_eq!(s.state, InteractionState::Streaming);
    assert!(s.is_streaming());
}

#[test]
fn enter_idle_empty_editor_is_noop() {
    let mut s = screen_for(7, true, InteractionState::Idle);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.state, InteractionState::Idle);
}

#[test]
fn shift_enter_inserts_newline_does_not_send() {
    let mut s = screen_for(7, true, InteractionState::Idle);
    type_text(&mut s, "line1");
    let action = s.handle_input(
        &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
        InputMode::Insert,
    );
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.editor_text(), "line1\n");
    assert_eq!(s.state, InteractionState::Idle);
}

#[test]
fn ctrl_p_produce_pr_true_sends_pushup_prompt() {
    let mut s = screen_for(42, true, InteractionState::Idle);
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
    let mut s = screen_for(42, false, InteractionState::Idle);
    let action = s.handle_input(&ctrl('p'), InputMode::Insert);
    match action {
        ScreenAction::LogActivity { tag, message, .. } => {
            assert_eq!(tag, "INTERACTION");
            assert!(message.contains("disabled"), "got: {message}");
        }
        other => panic!("expected LogActivity, got {other:?}"),
    }
    assert_eq!(s.state, InteractionState::Idle);
}

#[test]
fn ctrl_l_clears_editor_leaves_history() {
    let mut s = InteractionScreen::for_session(&{
        let mut sess = session_for(7, true, InteractionState::Idle);
        sess.history = three_turn_fixture();
        sess
    });
    type_text(&mut s, "partial");
    let action = s.handle_input(&ctrl('l'), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.editor_text(), "");
    assert_eq!(s.history.len(), 3);
}

#[test]
fn esc_idle_returns_pop() {
    let mut s = screen_for(7, true, InteractionState::Idle);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::Pop);
    assert!(!s.quit_modal_open);
}

#[test]
fn esc_streaming_returns_pop() {
    let mut s = screen_for(7, true, InteractionState::Streaming);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn streaming_ignores_enter() {
    let mut s = screen_for(7, true, InteractionState::Streaming);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.state, InteractionState::Streaming);
}

#[test]
fn streaming_ignores_ctrl_l() {
    let mut s = screen_for(7, true, InteractionState::Streaming);
    let action = s.handle_input(&ctrl('l'), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn streaming_scroll_up_still_works() {
    let mut s = InteractionScreen::for_session(&{
        let mut sess = session_for(7, true, InteractionState::Streaming);
        sess.history = three_turn_fixture();
        sess
    });
    s.scroll_offset = 5;
    s.auto_scroll = false;
    s.handle_input(&key_event(KeyCode::Up), InputMode::Insert);
    assert_eq!(s.scroll_offset, 4);
}

#[test]
fn terminated_any_key_returns_pop() {
    let mut s = screen_for(7, true, InteractionState::Terminated);
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
fn ctrl_q_opens_quit_modal() {
    let mut s = screen_for(7, true, InteractionState::Idle);
    let action = s.handle_input(&ctrl('q'), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(s.quit_modal_open);
}

#[test]
fn ctrl_q_streaming_opens_quit_modal() {
    let mut s = screen_for(7, true, InteractionState::Streaming);
    s.handle_input(&ctrl('q'), InputMode::Insert);
    assert!(s.quit_modal_open);
    assert_eq!(s.state, InteractionState::Streaming);
}

#[test]
fn quit_modal_y_terminates_and_returns_quit_action() {
    let mut s = screen_for(9, true, InteractionState::Idle);
    s.handle_input(&ctrl('q'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Char('y')), InputMode::Insert);
    assert_eq!(action, ScreenAction::QuitInteraction { issue_number: 9 });
    assert_eq!(s.state, InteractionState::Terminated);
    assert_eq!(s.close_reason, Some(CloseReason::UserQuit));
    assert!(!s.quit_modal_open);
}

#[test]
fn quit_modal_uppercase_y_terminates() {
    let mut s = screen_for(9, true, InteractionState::Idle);
    s.handle_input(&ctrl('q'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Char('Y')), InputMode::Insert);
    assert_eq!(action, ScreenAction::QuitInteraction { issue_number: 9 });
    assert_eq!(s.state, InteractionState::Terminated);
}

#[test]
fn quit_modal_n_cancels() {
    let mut s = screen_for(9, true, InteractionState::Idle);
    s.handle_input(&ctrl('q'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Char('n')), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(!s.quit_modal_open);
    assert_eq!(s.state, InteractionState::Idle);
}

#[test]
fn quit_modal_esc_cancels_modal_not_screen() {
    let mut s = screen_for(9, true, InteractionState::Idle);
    s.handle_input(&ctrl('q'), InputMode::Insert);
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(!s.quit_modal_open);
}

// --- keybindings provider ---

#[test]
fn keybindings_list_expected_keys() {
    let s = screen_for(7, true, InteractionState::Idle);
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
        "Ctrl+Q",
        "Esc",
        "Up/Down",
    ] {
        assert!(keys.contains(&expected), "missing {expected} in {keys:?}");
    }
}

#[test]
fn keybindings_ctrl_p_greyed_when_produce_pr_false() {
    let s = screen_for(7, false, InteractionState::Idle);
    let desc = s
        .keybindings()
        .into_iter()
        .flat_map(|g| g.bindings)
        .find(|b| b.key == "Ctrl+P")
        .map(|b| b.description)
        .unwrap_or("");
    assert!(desc.contains("greyed"), "got: {desc}");
}

// --- apply_turn_event ---

#[test]
fn apply_turn_started_pushes_agent_turn_and_streams() {
    let mut s = screen_for(7, true, InteractionState::Idle);
    let at = fixed_t0();
    let action = s.apply_turn_event(&TurnEvent::TurnStarted {
        role: TurnRole::Agent,
        at,
    });
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.state, InteractionState::Streaming);
    assert_eq!(s.history.len(), 1);
    assert_eq!(s.history[0].role, TurnRole::Agent);
    assert!(s.history[0].finished_at.is_none());
}

#[test]
fn apply_chunk_appends_to_streaming_agent_turn() {
    let mut s = screen_for(7, true, InteractionState::Idle);
    s.apply_turn_event(&TurnEvent::TurnStarted {
        role: TurnRole::Agent,
        at: fixed_t0(),
    });
    s.apply_turn_event(&TurnEvent::Chunk("hello ".into()));
    s.apply_turn_event(&TurnEvent::Chunk("world".into()));
    assert_eq!(s.history.last().unwrap().content, "hello world");
    assert_eq!(s.state, InteractionState::Streaming);
}

#[test]
fn apply_turn_finished_transitions_idle_and_logs_chunk_count() {
    let mut s = screen_for(13, true, InteractionState::Idle);
    // Simulate a send so turn_count == 1.
    type_text(&mut s, "hi");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let start = fixed_t0();
    s.apply_turn_event(&TurnEvent::TurnStarted {
        role: TurnRole::Agent,
        at: start,
    });
    s.apply_turn_event(&TurnEvent::Chunk("a".into()));
    s.apply_turn_event(&TurnEvent::Chunk("b".into()));
    let finished = start + chrono::Duration::milliseconds(250);
    let action = s.apply_turn_event(&TurnEvent::TurnFinished { at: finished });
    assert_eq!(s.state, InteractionState::Idle);
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
fn apply_error_appends_system_turn_and_returns_idle() {
    let mut s = screen_for(7, true, InteractionState::Streaming);
    s.apply_turn_event(&TurnEvent::TurnStarted {
        role: TurnRole::Agent,
        at: fixed_t0(),
    });
    let action = s.apply_turn_event(&TurnEvent::Error("agent exit 1".into()));
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.state, InteractionState::Idle);
    let last = s.history.last().unwrap();
    assert_eq!(last.role, TurnRole::System);
    assert!(last.content.contains("agent exit 1"));
}
