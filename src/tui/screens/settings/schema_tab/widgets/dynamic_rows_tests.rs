#![cfg(test)]
//! Behavioral and state-machine tests for [`super::DynamicRowsWidget`].

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::clock::{Clock, FakeClock};
use super::dynamic_rows::{DynamicRowsWidget, RowFocus};
use super::test_fixture::TEST_COMMAND_FIELDS;

fn alt_up() -> KeyEvent {
    KeyEvent {
        code: KeyCode::Up,
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn alt_down() -> KeyEvent {
    KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ev(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn typed_seq(w: &mut DynamicRowsWidget, s: &str) {
    for c in s.chars() {
        w.handle_input(ev(KeyCode::Char(c)));
    }
}

fn fresh() -> (DynamicRowsWidget, Arc<FakeClock>) {
    let clock: Arc<FakeClock> = Arc::new(FakeClock::new());
    let w = DynamicRowsWidget::with_clock(
        "completion_gates.commands",
        "completion_gates.commands",
        TEST_COMMAND_FIELDS,
        None,
        clock.clone() as Arc<dyn Clock>,
    );
    (w, clock)
}

fn add_n(w: &mut DynamicRowsWidget, n: usize) {
    for i in 0..n {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(w, &format!("row{}", i));
        w.handle_input(ev(KeyCode::Enter));
    }
}

#[test]
fn starts_empty() {
    let (w, _) = fresh();
    assert!(w.rows().is_empty());
    assert_eq!(*w.focus(), RowFocus::Empty);
}

#[test]
fn add_then_remove_via_modal() {
    let (mut w, _) = fresh();
    add_n(&mut w, 1);
    assert_eq!(w.rows().len(), 1);
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    assert!(w.rows().is_empty());
    assert!(w.undo_active());
}

#[test]
fn alt_up_at_zero_is_noop() {
    let (mut w, _) = fresh();
    add_n(&mut w, 2);
    w.handle_input(ev(KeyCode::Up));
    assert_eq!(w.focused_row(), Some(0));
    w.handle_input(alt_up());
    assert_eq!(w.focused_row(), Some(0));
}

#[test]
fn alt_down_at_last_is_noop() {
    let (mut w, _) = fresh();
    add_n(&mut w, 2);
    assert_eq!(w.focused_row(), Some(1));
    w.handle_input(alt_down());
    assert_eq!(w.focused_row(), Some(1));
}

#[test]
fn alt_up_swaps_with_previous_and_focus_follows() {
    let (mut w, _) = fresh();
    add_n(&mut w, 3);
    let before: Vec<String> = w.rows().iter().map(|e| e.id.clone()).collect();
    assert_eq!(before, vec!["row0", "row1", "row2"]);
    w.handle_input(alt_up());
    let after: Vec<String> = w.rows().iter().map(|e| e.id.clone()).collect();
    assert_eq!(after, vec!["row0", "row2", "row1"]);
    assert_eq!(w.focused_row(), Some(1));
}

#[test]
fn alt_down_swaps_with_next_and_focus_follows() {
    let (mut w, _) = fresh();
    add_n(&mut w, 3);
    w.handle_input(ev(KeyCode::Up));
    w.handle_input(ev(KeyCode::Up));
    assert_eq!(w.focused_row(), Some(0));
    w.handle_input(alt_down());
    let after: Vec<String> = w.rows().iter().map(|e| e.id.clone()).collect();
    assert_eq!(after, vec!["row1", "row0", "row2"]);
    assert_eq!(w.focused_row(), Some(1));
}

#[test]
fn undo_restores_row_at_original_index() {
    let (mut w, clock) = fresh();
    add_n(&mut w, 3);
    w.handle_input(ev(KeyCode::Up));
    assert_eq!(w.focused_row(), Some(1));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    let after_delete: Vec<String> = w.rows().iter().map(|e| e.id.clone()).collect();
    assert_eq!(after_delete, vec!["row0", "row2"]);
    clock.advance(Duration::from_secs(1));
    w.handle_input(ev(KeyCode::Char('u')));
    let restored: Vec<String> = w.rows().iter().map(|e| e.id.clone()).collect();
    assert_eq!(restored, vec!["row0", "row1", "row2"]);
    assert_eq!(w.focused_row(), Some(1));
}

#[test]
fn serialize_to_toml_produces_array() {
    let (mut w, _) = fresh();
    add_n(&mut w, 2);
    let v = w.serialize_to_toml();
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2);
}

#[test]
fn d_with_no_rows_is_noop() {
    let (mut w, _) = fresh();
    w.handle_input(ev(KeyCode::Char('d')));
    assert!(w.rows().is_empty());
}
