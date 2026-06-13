//! Unit tests for the Interaction screen state machine (#736).

use super::layout::{effective_offset, inset_x};
use super::view_state::InteractionView;
use super::*;
use crate::session::interaction::TurnRole;
use crate::session::types::SessionStatus;
use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
use chrono::{DateTime, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};

fn fixed_t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap()
}

#[test]
fn inset_x_trims_both_sides_and_keeps_vertical() {
    let area = Rect {
        x: 0,
        y: 3,
        width: 120,
        height: 40,
    };
    let r = inset_x(area, 1);
    assert_eq!(r.x, 1, "shifts right by margin");
    assert_eq!(r.width, 118, "trims margin off each side");
    assert_eq!(r.y, 3, "vertical unchanged");
    assert_eq!(r.height, 40, "vertical unchanged");
}

#[test]
fn inset_x_saturates_on_narrow_area() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 1,
        height: 5,
    };
    // 2*margin > width must not underflow.
    let r = inset_x(area, 1);
    assert_eq!(r.width, 0);
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

// --- #988: page / jump-to-latest math ---

#[test]
fn page_up_pages_by_one_viewport_and_disables_auto_scroll() {
    let mut s = InteractionScreen::new();
    s.last_viewport = 10;
    s.last_max_offset = 50;
    s.scroll_offset = 30;
    s.page_up();
    assert_eq!(s.scroll_offset, 20, "paged up by one viewport height");
    assert!(!s.auto_scroll, "paging up takes manual control");
}

#[test]
fn page_up_clamps_at_top() {
    let mut s = InteractionScreen::new();
    s.last_viewport = 10;
    s.last_max_offset = 50;
    s.scroll_offset = 5;
    s.page_up();
    assert_eq!(s.scroll_offset, 0, "must not underflow past the top");
}

#[test]
fn page_down_clamps_at_bottom_and_re_pins() {
    let mut s = InteractionScreen::new();
    s.last_viewport = 10;
    s.last_max_offset = 50;
    s.auto_scroll = false;
    s.scroll_offset = 45;
    s.page_down();
    assert_eq!(s.scroll_offset, 50, "clamped at last_max_offset");
    assert!(s.auto_scroll, "reaching the bottom re-pins tail-following");
}

#[test]
fn page_with_zero_viewport_still_moves_one_line() {
    // Before the first draw `last_viewport` is 0; `.max(1)` keeps paging alive.
    let mut s = InteractionScreen::new();
    s.last_max_offset = 50;
    s.scroll_offset = 5;
    s.page_up();
    assert_eq!(s.scroll_offset, 4);
}

#[test]
fn jump_to_latest_pins_to_bottom() {
    let mut s = InteractionScreen::new();
    s.last_max_offset = 42;
    s.auto_scroll = false;
    s.scroll_offset = 3;
    s.jump_to_latest();
    assert!(s.auto_scroll, "End resumes tail-following");
    assert_eq!(s.scroll_offset, 42, "End jumps to the newest line");
}

fn streaming_view(turns: Vec<TurnRecord>) -> InteractionView {
    InteractionView {
        turns,
        turn_state: crate::session::interaction::TurnState::Streaming,
        settled_from: None,
        pr_linked: None,
    }
}

#[test]
fn set_view_while_scrolled_up_does_not_yank_the_view() {
    // #950: a view refresh (a chunk landed on the session) must not reset
    // auto_scroll or move scroll_offset.
    let mut s = InteractionScreen::with_history(three_turn_fixture());
    s.auto_scroll = false;
    s.scroll_offset = 2;

    let mut grown = three_turn_fixture();
    grown.push(user_turn("streamed text"));
    s.set_view(streaming_view(grown));

    assert!(!s.auto_scroll, "a view refresh must not re-pin the tail");
    assert_eq!(s.scroll_offset, 2, "a view refresh must not move scroll");
}

#[test]
fn set_view_replaces_transcript_each_frame() {
    // The injected view is the source of truth — a later view wins; turns
    // never accumulate on the screen.
    let mut s = InteractionScreen::new();
    s.set_view(streaming_view(vec![user_turn("one")]));
    assert_eq!(s.history_len(), 1);
    s.set_view(streaming_view(three_turn_fixture()));
    assert_eq!(s.history_len(), 3);
}

#[test]
fn set_view_drives_lock_and_handles_empty() {
    let mut s = InteractionScreen::new();
    s.set_view(InteractionView::default());
    assert_eq!(s.history_len(), 0, "empty view does not panic");
    assert!(!s.is_streaming());
    s.set_view(streaming_view(Vec::new()));
    assert!(s.is_streaming(), "TurnState::Streaming locks input");
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

#[test]
fn long_and_empty_content_turns_in_view_do_not_panic() {
    let mut s = InteractionScreen::new();
    s.set_view(streaming_view(vec![
        TurnRecord {
            role: TurnRole::User,
            content: String::new(),
            started_at: fixed_t0(),
            finished_at: None,
        },
        user_turn(&"x".repeat(10_000)),
    ]));
    assert_eq!(s.history_len(), 2);
}

#[test]
fn banner_reflects_settled_from_and_pr_linked() {
    // #950 headline: the status banner is derived from the injected view.
    let mut s = InteractionScreen::new();
    assert_eq!(s.banner(), None, "no banner until the session settles");
    s.set_view(InteractionView {
        turns: Vec::new(),
        turn_state: crate::session::interaction::TurnState::Idle,
        settled_from: Some(SessionStatus::Completed),
        pr_linked: Some(42),
    });
    let banner = s.banner().expect("settled view has a banner");
    assert!(banner.contains("COMPLETED"), "got: {banner}");
    assert!(banner.contains("PR #42"), "got: {banner}");
}
