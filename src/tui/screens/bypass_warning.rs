//! First-time danger-warning modal for bypass mode (#328).
//!
//! The user must literally type `CONFIRM` to enable bypass mode the first
//! time they activate it in a session. Any other input cancels.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #328. The screen is routed from `app.rs`
// when the user first activates bypass in Phase 2; tests exercise the
// CONFIRM-typing state machine today.
#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyEvent};

const CONFIRM_PHRASE: &str = "CONFIRM";

pub const WARNING_TEXT: &str = "\
╔══════════════════════════════════════════════════════════════════╗
║  ⚠  DANGER — BYPASS MODE                                          ║
║                                                                  ║
║  Bypass mode auto-accepts EVERY review concern without           ║
║  confirmation, edits files, commits, and pushes.                 ║
║                                                                  ║
║  Use only when you trust the reviewer fully.                     ║
║                                                                  ║
║  Type 'CONFIRM' to enable for THIS session only.                 ║
║  Press Esc to cancel.                                            ║
╚══════════════════════════════════════════════════════════════════╝
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BypassWarningState {
    AwaitingConfirmation { typed: String },
    Confirmed,
    Cancelled,
}

impl BypassWarningState {
    pub fn new() -> Self {
        Self::AwaitingConfirmation {
            typed: String::new(),
        }
    }
}

impl Default for BypassWarningState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassWarningOutcome {
    Confirmed,
    Cancelled,
    Pending,
}

pub fn handle_key(state: &mut BypassWarningState, key: KeyEvent) -> BypassWarningOutcome {
    if matches!(key.code, KeyCode::Esc) {
        *state = BypassWarningState::Cancelled;
        return BypassWarningOutcome::Cancelled;
    }

    let BypassWarningState::AwaitingConfirmation { typed } = state else {
        return current_outcome(state);
    };

    let KeyCode::Char(c) = key.code else {
        return BypassWarningOutcome::Pending;
    };

    typed.push(c);

    let next_index = typed.len();
    let expected = &CONFIRM_PHRASE[..next_index.min(CONFIRM_PHRASE.len())];

    if !typed.starts_with(expected) {
        *state = BypassWarningState::Cancelled;
        return BypassWarningOutcome::Cancelled;
    }

    if typed == CONFIRM_PHRASE {
        *state = BypassWarningState::Confirmed;
        return BypassWarningOutcome::Confirmed;
    }

    BypassWarningOutcome::Pending
}

fn current_outcome(state: &BypassWarningState) -> BypassWarningOutcome {
    match state {
        BypassWarningState::Confirmed => BypassWarningOutcome::Confirmed,
        BypassWarningState::Cancelled => BypassWarningOutcome::Cancelled,
        BypassWarningState::AwaitingConfirmation { .. } => BypassWarningOutcome::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn esc() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    #[test]
    fn typing_confirm_sequentially_confirms() {
        let mut state = BypassWarningState::new();
        let mut last = BypassWarningOutcome::Pending;
        for c in "CONFIRM".chars() {
            last = handle_key(&mut state, key(c));
        }
        assert_eq!(last, BypassWarningOutcome::Confirmed);
        assert_eq!(state, BypassWarningState::Confirmed);
    }

    #[test]
    fn wrong_first_char_cancels() {
        let mut state = BypassWarningState::new();
        let outcome = handle_key(&mut state, key('X'));
        assert_eq!(outcome, BypassWarningOutcome::Cancelled);
        assert_eq!(state, BypassWarningState::Cancelled);
    }

    #[test]
    fn partial_then_wrong_cancels() {
        let mut state = BypassWarningState::new();
        handle_key(&mut state, key('C'));
        handle_key(&mut state, key('O'));
        let outcome = handle_key(&mut state, key('X'));
        assert_eq!(outcome, BypassWarningOutcome::Cancelled);
    }

    #[test]
    fn esc_at_start_cancels() {
        let mut state = BypassWarningState::new();
        let outcome = handle_key(&mut state, esc());
        assert_eq!(outcome, BypassWarningOutcome::Cancelled);
    }

    #[test]
    fn esc_mid_typing_cancels() {
        let mut state = BypassWarningState::new();
        handle_key(&mut state, key('C'));
        handle_key(&mut state, key('O'));
        let outcome = handle_key(&mut state, esc());
        assert_eq!(outcome, BypassWarningOutcome::Cancelled);
    }

    #[test]
    fn lowercase_input_cancels() {
        let mut state = BypassWarningState::new();
        // The literal phrase is uppercase. Lowercase 'c' breaks the prefix.
        let outcome = handle_key(&mut state, key('c'));
        assert_eq!(outcome, BypassWarningOutcome::Cancelled);
    }

    #[test]
    fn non_char_keys_are_ignored_until_esc() {
        let mut state = BypassWarningState::new();
        let outcome = handle_key(&mut state, KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
        assert_eq!(outcome, BypassWarningOutcome::Pending);
        assert_eq!(
            state,
            BypassWarningState::AwaitingConfirmation {
                typed: String::new()
            }
        );
    }

    #[test]
    fn confirmed_state_is_terminal() {
        let mut state = BypassWarningState::Confirmed;
        let outcome = handle_key(&mut state, key('X'));
        assert_eq!(outcome, BypassWarningOutcome::Confirmed);
        assert_eq!(state, BypassWarningState::Confirmed);
    }

    #[test]
    fn warning_text_mentions_session_only() {
        assert!(WARNING_TEXT.contains("THIS session"));
        assert!(WARNING_TEXT.contains("CONFIRM"));
        assert!(WARNING_TEXT.contains("DANGER"));
    }
}
