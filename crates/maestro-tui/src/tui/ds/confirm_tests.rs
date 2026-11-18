use super::*;
use crossterm::event::{KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(confirm: &mut TypedConfirm, s: &str) {
    for c in s.chars() {
        assert_eq!(
            confirm.handle_key(key(KeyCode::Char(c))),
            ConfirmOutcome::Pending
        );
    }
}

fn kill_407() -> TypedConfirm {
    TypedConfirm::new(
        "Kill session #407?",
        "407",
        "kill",
        vec![("kept".into(), "the worktree".into())],
    )
}

#[test]
fn typed_confirm_enter_inert_until_match() {
    let mut confirm = kill_407();
    assert_eq!(
        confirm.handle_key(key(KeyCode::Enter)),
        ConfirmOutcome::Pending
    );
    type_str(&mut confirm, "40");
    assert!(!confirm.matches());
    assert_eq!(
        confirm.handle_key(key(KeyCode::Enter)),
        ConfirmOutcome::Pending
    );
    type_str(&mut confirm, "77");
    assert_eq!(confirm.typed(), "4077");
    assert_eq!(
        confirm.handle_key(key(KeyCode::Enter)),
        ConfirmOutcome::Pending
    );
    confirm.handle_key(key(KeyCode::Backspace));
    assert!(confirm.matches());
    assert_eq!(
        confirm.handle_key(key(KeyCode::Enter)),
        ConfirmOutcome::Confirmed
    );
}

#[test]
fn esc_cancels_at_any_point() {
    let mut confirm = kill_407();
    assert_eq!(
        confirm.handle_key(key(KeyCode::Esc)),
        ConfirmOutcome::Cancelled
    );
    let mut confirm = kill_407();
    type_str(&mut confirm, "407");
    assert_eq!(
        confirm.handle_key(key(KeyCode::Esc)),
        ConfirmOutcome::Cancelled
    );
}

#[test]
fn match_is_exact_and_case_sensitive() {
    let mut confirm = TypedConfirm::new("Enable bypass?", "CONFIRM", "enable", vec![]);
    type_str(&mut confirm, "confirm");
    assert!(!confirm.matches());
    assert_eq!(
        confirm.handle_key(key(KeyCode::Enter)),
        ConfirmOutcome::Pending
    );
}
