//! Unit tests for the Interaction screen state machine (#736).

use super::*;
use crate::session::interaction::TurnRole;
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

// --- effective_offset ---

#[test]
fn effective_offset_empty_history_returns_zero() {
    assert_eq!(effective_offset(true, 0, 0, 10), 0);
}

#[test]
fn effective_offset_auto_scroll_follows_max() {
    assert_eq!(effective_offset(true, 5, 20, 10), 10);
}

#[test]
fn effective_offset_pinned_below_max_is_unchanged() {
    assert_eq!(effective_offset(false, 6, 20, 10), 6);
}

#[test]
fn effective_offset_pinned_clamps_at_max() {
    assert_eq!(effective_offset(false, 99, 20, 10), 10);
}

#[test]
fn effective_offset_pinned_at_zero_stays_zero() {
    assert_eq!(effective_offset(false, 0, 5, 10), 0);
}

#[test]
fn effective_offset_viewport_larger_than_total_max_is_zero() {
    assert_eq!(effective_offset(true, 0, 3, 10), 0);
}

#[test]
fn effective_offset_viewport_equal_to_total_max_is_zero() {
    assert_eq!(effective_offset(true, 0, 10, 10), 0);
}

// --- scroll state machine ---

#[test]
fn new_auto_scroll_is_true() {
    let s = InteractionScreen::new();
    assert!(s.auto_scroll);
    assert_eq!(s.scroll_offset, 0);
}

#[test]
fn scroll_up_sets_auto_scroll_false() {
    let mut s = InteractionScreen::new();
    s.scroll_up_for_test(1);
    assert!(!s.auto_scroll);
}

#[test]
fn scroll_up_decrements_offset() {
    let mut s = InteractionScreen::new();
    s.scroll_offset = 5;
    s.scroll_up_for_test(3);
    assert_eq!(s.scroll_offset, 2);
}

#[test]
fn scroll_up_saturates_at_zero() {
    let mut s = InteractionScreen::new();
    s.scroll_up_for_test(100);
    assert_eq!(s.scroll_offset, 0);
    assert!(!s.auto_scroll);
}

#[test]
fn scroll_up_on_empty_history_does_not_panic() {
    let mut s = InteractionScreen::new();
    s.scroll_up_for_test(1);
    assert_eq!(s.scroll_offset, 0);
}

#[test]
fn scroll_down_past_bottom_re_enables_auto_scroll() {
    let mut s = InteractionScreen::new();
    s.auto_scroll = false;
    s.scroll_offset = 0;
    s.scroll_down_for_test(1);
    assert!(s.auto_scroll);
}

#[test]
fn scroll_down_below_bottom_does_not_overflow_offset() {
    let mut s = InteractionScreen::new();
    s.auto_scroll = false;
    s.scroll_down_for_test(99);
    assert_eq!(s.scroll_offset, 0);
}

#[test]
fn scroll_down_already_at_bottom_re_pins_auto_scroll() {
    let mut s = InteractionScreen::with_history(three_turn_fixture());
    s.auto_scroll = false;
    s.scroll_offset = 999;
    s.scroll_down_for_test(1);
    assert!(s.auto_scroll);
}

#[test]
fn push_turn_appends_to_history() {
    let mut s = InteractionScreen::new();
    s.push_turn(user_turn("hello"));
    assert_eq!(s.history.len(), 1);
    assert_eq!(s.history[0].content, "hello");
}

#[test]
fn push_turn_preserves_scroll_offset_when_scrolled_up() {
    let mut s = InteractionScreen::with_history(three_turn_fixture());
    s.auto_scroll = false;
    s.scroll_offset = 2;
    s.push_turn(user_turn("new"));
    assert!(!s.auto_scroll);
    assert_eq!(s.scroll_offset, 2);
}

#[test]
fn with_history_starts_auto_scroll_true() {
    let s = InteractionScreen::with_history(vec![user_turn(&"x".repeat(500))]);
    assert!(s.auto_scroll);
}

// --- editor_text ---

#[test]
fn editor_text_empty_on_new_screen() {
    let s = InteractionScreen::new();
    assert_eq!(s.editor_text(), "");
}

// --- handle_input ---

#[test]
fn handle_input_esc_returns_pop() {
    let mut s = InteractionScreen::new();
    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn handle_input_up_calls_scroll_up() {
    let mut s = InteractionScreen::new();
    s.scroll_offset = 5;
    s.auto_scroll = false;
    s.handle_input(&key_event(KeyCode::Up), InputMode::Insert);
    assert_eq!(s.scroll_offset, 4);
    assert!(!s.auto_scroll);
}

#[test]
fn handle_input_down_calls_scroll_down() {
    let mut s = InteractionScreen::new();
    s.auto_scroll = false;
    s.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    assert!(s.auto_scroll);
}

#[test]
fn handle_input_char_key_feeds_editor() {
    let mut s = InteractionScreen::new();
    s.handle_input(&key_event(KeyCode::Char('h')), InputMode::Insert);
    s.handle_input(&key_event(KeyCode::Char('i')), InputMode::Insert);
    assert_eq!(s.editor_text(), "hi");
}

#[test]
fn handle_input_char_key_returns_none() {
    let mut s = InteractionScreen::new();
    let action = s.handle_input(&key_event(KeyCode::Char('x')), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn handle_input_up_returns_none() {
    let mut s = InteractionScreen::new();
    let action = s.handle_input(&key_event(KeyCode::Up), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn handle_input_release_event_is_ignored() {
    let mut s = InteractionScreen::new();
    let release = crossterm::event::Event::Key(crossterm::event::KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: crossterm::event::KeyEventState::NONE,
    });
    let action = s.handle_input(&release, InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(s.editor_text(), "");
}

#[test]
fn handle_input_modifier_char_still_feeds_editor() {
    let mut s = InteractionScreen::new();
    s.handle_input(
        &key_event_with_modifiers(KeyCode::Char('a'), KeyModifiers::SHIFT),
        InputMode::Insert,
    );
    assert_eq!(s.editor_text(), "a");
}

// --- trait ---

#[test]
fn desired_input_mode_returns_insert() {
    let s = InteractionScreen::new();
    assert_eq!(s.desired_input_mode(), Some(InputMode::Insert));
}

// --- edge cases ---

#[test]
fn turn_with_empty_content_does_not_panic_on_push() {
    let mut s = InteractionScreen::new();
    s.push_turn(TurnRecord {
        role: TurnRole::User,
        content: String::new(),
        started_at: fixed_t0(),
        finished_at: None,
    });
    assert_eq!(s.history.len(), 1);
    assert_eq!(s.history[0].content, "");
}

#[test]
fn very_long_single_turn_push_does_not_panic() {
    let long = "x".repeat(10_000);
    let mut s = InteractionScreen::new();
    s.push_turn(user_turn(&long));
    assert_eq!(s.history[0].content.len(), 10_000);
}
