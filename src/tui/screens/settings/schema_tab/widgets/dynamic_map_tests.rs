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
fn ctrl_right_advances_tab() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    w.handle_input(ev_ctrl(KeyCode::Right));
    assert_eq!(w.active_index(), Some(0));
}

#[test]
fn ctrl_left_moves_back() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    w.handle_input(ev_ctrl(KeyCode::Left));
    assert_eq!(w.active_index(), Some(0));
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
