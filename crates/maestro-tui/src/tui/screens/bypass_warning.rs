//! First-time danger-warning modal for bypass mode (#328).
//!
//! The user must type `CONFIRM`, then press `⏎`, to enable bypass mode the
//! first time they activate it in a session. `Esc` cancels (#1261).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]
// Reason: Phase 1 foundation for #328. The screen is routed from `app.rs`
// when the user first activates bypass in Phase 2; tests exercise the
// CONFIRM-typing state machine today.
#![allow(dead_code)]

use crossterm::event::KeyEvent;

use crate::tui::ds::confirm::TypedConfirm;

const CONFIRM_PHRASE: &str = "CONFIRM";

/// `Confirmed` / `Cancelled` / `Pending`, the same as every typed confirm.
pub use crate::tui::ds::confirm::ConfirmOutcome as BypassWarningOutcome;

/// The warning dialog: type `CONFIRM`, then `⏎` (#1261). Built on
/// `ds::confirm::TypedConfirm`.
pub struct BypassWarningState {
    pub confirm: TypedConfirm,
}

impl BypassWarningState {
    pub fn new() -> Self {
        let row = |label: &str, text: &str| (label.to_string(), text.to_string());
        Self {
            confirm: TypedConfirm::new(
                "DANGER: enable bypass mode?",
                CONFIRM_PHRASE,
                "enable",
                vec![
                    row("", "Bypass auto-accepts EVERY review concern."),
                    row("", ""),
                    row("does", "edits files, commits and pushes, no asking"),
                    row("scope", "THIS session only"),
                    row("trust", "use only when you trust the reviewer fully"),
                ],
            ),
        }
    }
}

impl Default for BypassWarningState {
    fn default() -> Self {
        Self::new()
    }
}

/// `⏎` confirms only once the typed text is `CONFIRM`; `Esc` cancels.
pub fn handle_key(state: &mut BypassWarningState, key: KeyEvent) -> BypassWarningOutcome {
    state.confirm.handle_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_str(state: &mut BypassWarningState, s: &str) {
        for c in s.chars() {
            assert_eq!(
                handle_key(state, key(KeyCode::Char(c))),
                BypassWarningOutcome::Pending
            );
        }
    }

    #[test]
    fn typing_confirm_then_enter_confirms() {
        let mut state = BypassWarningState::new();
        type_str(&mut state, "CONFIRM");
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Enter)),
            BypassWarningOutcome::Confirmed
        );
    }

    #[test]
    fn enter_before_the_word_is_inert() {
        let mut state = BypassWarningState::new();
        type_str(&mut state, "CONF");
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Enter)),
            BypassWarningOutcome::Pending
        );
    }

    #[test]
    fn lowercase_word_does_not_confirm() {
        let mut state = BypassWarningState::new();
        type_str(&mut state, "confirm");
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Enter)),
            BypassWarningOutcome::Pending
        );
    }

    #[test]
    fn esc_mid_typing_cancels() {
        let mut state = BypassWarningState::new();
        type_str(&mut state, "CO");
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Esc)),
            BypassWarningOutcome::Cancelled
        );
    }

    #[test]
    fn token_is_the_confirm_phrase() {
        assert_eq!(BypassWarningState::new().confirm.expected(), "CONFIRM");
    }
}
