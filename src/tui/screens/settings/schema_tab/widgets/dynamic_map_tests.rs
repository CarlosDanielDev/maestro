#![cfg(test)]
//! Behavioral and state-machine tests for [`super::DynamicMapWidget`].

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::clock::{Clock, FakeClock};
use super::dynamic_map::{DynamicMapWidget, MapFocus};
use super::test_fixture::TEST_AGENT_FIELDS;

fn ev(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ev_ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn typed_seq(w: &mut DynamicMapWidget, s: &str) {
    for c in s.chars() {
        w.handle_input(ev(KeyCode::Char(c)));
    }
}

fn fresh_with_clock() -> (DynamicMapWidget, Arc<FakeClock>) {
    let clock: Arc<FakeClock> = Arc::new(FakeClock::new());
    let w = DynamicMapWidget::with_clock(
        "agents.entries",
        "agents",
        TEST_AGENT_FIELDS,
        None,
        clock.clone() as Arc<dyn Clock>,
    );
    (w, clock)
}

#[test]
fn starts_empty() {
    let (w, _) = fresh_with_clock();
    assert!(w.entries().is_empty());
    assert!(w.active_entry().is_none());
    assert_eq!(*w.focus(), MapFocus::SubtabStrip);
}

#[test]
fn pressing_a_opens_add_modal() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    assert_eq!(*w.focus(), MapFocus::AddModal);
}

#[test]
fn add_modal_submit_inserts_entry() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(w.entries().len(), 1);
    assert_eq!(w.entries()[0].id, "claude");
}

#[test]
fn add_modal_submit_focuses_first_editable_field() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
}

#[test]
fn add_modal_invalid_id_keeps_modal_open() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "X");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::AddModal);
    assert!(w.entries().is_empty());
}

#[test]
fn add_modal_collision_keeps_modal_open() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(w.entries().len(), 1);
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::AddModal);
    assert_eq!(w.entries().len(), 1);
}

#[test]
fn bracket_right_advances_tab() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    // After Submit, focus is on EntryField(0); step Up to the subtab strip
    // so `]` is interpreted as next-tab and not delegated to the inner field.
    w.handle_input(ev(KeyCode::Up));
    w.handle_input(ev(KeyCode::Char(']')));
    assert_eq!(w.active_index(), Some(0));
}

#[test]
fn bracket_left_moves_back() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    w.handle_input(ev(KeyCode::Up));
    w.handle_input(ev(KeyCode::Char('[')));
    assert_eq!(w.active_index(), Some(0));
}

#[test]
fn bracket_ignored_when_entry_field_focused() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    // Focus is on EntryField(0); `]` must be delegated to the inner widget
    // (so users can type the literal bracket into a text field) instead of
    // switching tabs.
    let before = w.active_index();
    w.handle_input(ev(KeyCode::Char(']')));
    assert_eq!(w.active_index(), before);
}

#[test]
fn d_typed_into_text_field_does_not_open_remove_modal() {
    // Regression: user types "opencode" into a String field; the inner
    // 'd' should land in the TextInput buffer, NOT trigger the Remove-
    // entry shortcut. Pre-fix, DynamicMap.handle_input matched
    // Char('d') before delegating, so the modal opened mid-word.
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    // Focus is EntryField(0). Walk Down to a String field — TEST_AGENT_FIELDS
    // index 2 is `command` (String). 0 = kind (Enum), 1 = enabled (Bool),
    // 2 = command (String).
    w.handle_input(ev(KeyCode::Down));
    w.handle_input(ev(KeyCode::Down));
    assert_eq!(*w.focus(), MapFocus::EntryField(2));
    // Enter insert mode on the TextInput.
    w.handle_input(ev(KeyCode::Enter));
    // Type "opencode" — the `d` and `a` must stay characters, not open
    // Remove / Add modals.
    typed_seq(&mut w, "opencode");
    assert!(
        w.add_modal().is_none(),
        "`a` typed while editing must not open Add modal"
    );
    assert!(
        w.remove_modal().is_none(),
        "`d` typed while editing must not open Remove modal"
    );
}

#[test]
fn down_advances_through_entry_fields() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "alpha");
    w.handle_input(ev(KeyCode::Enter));
    // Submit puts focus on EntryField(0); Down walks to (1), (2), ... until
    // saturated.
    let len = w.entry_fields.len();
    assert!(len >= 2);
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
    w.handle_input(ev(KeyCode::Down));
    assert_eq!(*w.focus(), MapFocus::EntryField(1));
    // Saturates at the last entry-field index.
    for _ in 0..(len + 4) {
        w.handle_input(ev(KeyCode::Down));
    }
    assert_eq!(*w.focus(), MapFocus::EntryField(len - 1));
}

#[test]
fn up_from_first_entry_field_returns_to_subtab_strip() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "alpha");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
    w.handle_input(ev(KeyCode::Up));
    assert_eq!(*w.focus(), MapFocus::SubtabStrip);
}

#[test]
fn remove_then_undo_restores() {
    let (mut w, clock) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    assert!(w.entries().is_empty());
    assert!(w.undo_active());
    clock.advance(Duration::from_secs(2));
    w.handle_input(ev(KeyCode::Char('u')));
    assert_eq!(w.entries().len(), 1);
    assert_eq!(w.entries()[0].id, "claude");
}

#[test]
fn remove_then_undo_after_window_is_noop() {
    let (mut w, clock) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    clock.advance(Duration::from_secs(6));
    assert!(!w.undo_active());
    w.handle_input(ev(KeyCode::Char('u')));
    assert!(w.entries().is_empty());
}

#[test]
fn remove_cancel_keeps_entry() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('n')));
    assert_eq!(w.entries().len(), 1);
}

#[test]
fn serialize_to_toml_emits_keyed_table() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    let v = w.serialize_to_toml();
    let t = v.as_table().expect("table");
    assert!(t.contains_key("claude"));
}

#[test]
fn d_with_no_entries_is_noop() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('d')));
    assert_eq!(*w.focus(), MapFocus::SubtabStrip);
}

#[test]
fn edit_hint_is_static() {
    let (w, _) = fresh_with_clock();
    let (k, l) = w.edit_hint();
    assert!(!k.is_empty());
    assert!(!l.is_empty());
}
