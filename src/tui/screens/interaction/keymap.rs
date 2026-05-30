//! Keymap classification for the Interaction screen (#738).
//!
//! Pure decision layer: maps `(state, produce_pr, key, modifiers)` to an
//! [`InteractionIntent`] before any mutation. Keeping it a free function
//! makes the keymap exhaustively unit-testable (RUST-GUARDRAILS §7) and keeps
//! `handle_input` at one level of indentation.

use crate::session::interaction::InteractionState;
use crossterm::event::{KeyCode, KeyModifiers};

/// What a key press means on the Interaction screen, resolved from the
/// current state and the launch-time `produce_pr` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InteractionIntent {
    /// `Enter` in `Idle` with a non-empty buffer — send the input as a turn.
    SendInput,
    /// `Shift+Enter` — insert a newline; do not send.
    InsertNewline,
    /// `Ctrl+P` in `Idle` with `produce_pr` — send the hard-coded pushup prompt.
    SendPushup,
    /// `Ctrl+P` when `produce_pr` is false — greyed; log and ignore.
    PushupDisabled,
    /// `Ctrl+L` in `Idle` — clear the input buffer (history untouched).
    ClearInput,
    /// `Esc` — return to the Issues list (honoured in every state).
    Back,
    /// `Ctrl+Q` — open the quit-confirm modal.
    RequestQuit,
    /// `Up` — scroll history up (any state).
    ScrollUp,
    /// `Down` — scroll history down (any state).
    ScrollDown,
    /// Any other key in `Idle` — feed it to the text editor.
    FeedEditor,
    /// A send/edit key while `Streaming` — input locked, ignore.
    Locked,
}

/// Classify one key press. Order matters: terminal/global keys resolve before
/// the streaming lock so `Esc`, `Ctrl+Q`, and scroll still work mid-stream.
pub(crate) fn classify(
    state: InteractionState,
    produce_pr: bool,
    code: KeyCode,
    mods: KeyModifiers,
) -> InteractionIntent {
    use InteractionIntent::*;

    // A terminated session leaves on any key.
    if state == InteractionState::Terminated {
        return Back;
    }

    // Global keys resolve before the streaming lock so the user can still
    // leave, quit, or scroll while a turn streams.
    match code {
        KeyCode::Esc => return Back,
        KeyCode::Up => return ScrollUp,
        KeyCode::Down => return ScrollDown,
        _ => {}
    }
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    if ctrl && code == KeyCode::Char('q') {
        return RequestQuit;
    }

    // While streaming, every remaining send/edit key is ignored.
    if state == InteractionState::Streaming {
        return Locked;
    }

    // Idle: the active editing keymap.
    if code == KeyCode::Enter {
        return if mods.contains(KeyModifiers::SHIFT) {
            InsertNewline
        } else {
            SendInput
        };
    }
    if ctrl && code == KeyCode::Char('p') {
        return if produce_pr {
            SendPushup
        } else {
            PushupDisabled
        };
    }
    if ctrl && code == KeyCode::Char('l') {
        return ClearInput;
    }
    FeedEditor
}

/// The hard-coded pushup prompt sent by `Ctrl+P`. Centralised so the screen
/// and tests share one source (RUST-GUARDRAILS §12).
pub(crate) fn pushup_prompt(issue_number: u64) -> String {
    format!("Use the /pushup skill to commit, push, and open a PR for issue #{issue_number}.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use InteractionIntent::*;
    use InteractionState::{Idle, Streaming, Terminated};

    fn ctrl(c: char) -> (KeyCode, KeyModifiers) {
        (KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn enter_idle_sends_input() {
        assert_eq!(
            classify(Idle, true, KeyCode::Enter, KeyModifiers::NONE),
            SendInput
        );
    }

    #[test]
    fn enter_streaming_is_locked() {
        assert_eq!(
            classify(Streaming, true, KeyCode::Enter, KeyModifiers::NONE),
            Locked
        );
    }

    #[test]
    fn any_key_terminated_navigates_back() {
        assert_eq!(
            classify(Terminated, true, KeyCode::Enter, KeyModifiers::NONE),
            Back
        );
        assert_eq!(
            classify(Terminated, true, KeyCode::Char('x'), KeyModifiers::NONE),
            Back
        );
    }

    #[test]
    fn shift_enter_inserts_newline() {
        assert_eq!(
            classify(Idle, true, KeyCode::Enter, KeyModifiers::SHIFT),
            InsertNewline
        );
    }

    #[test]
    fn ctrl_p_idle_with_produce_pr_sends_pushup() {
        let (c, m) = ctrl('p');
        assert_eq!(classify(Idle, true, c, m), SendPushup);
    }

    #[test]
    fn ctrl_p_idle_without_produce_pr_is_disabled() {
        let (c, m) = ctrl('p');
        assert_eq!(classify(Idle, false, c, m), PushupDisabled);
    }

    #[test]
    fn ctrl_p_streaming_is_locked() {
        let (c, m) = ctrl('p');
        assert_eq!(classify(Streaming, true, c, m), Locked);
    }

    #[test]
    fn ctrl_l_idle_clears_input() {
        let (c, m) = ctrl('l');
        assert_eq!(classify(Idle, true, c, m), ClearInput);
    }

    #[test]
    fn ctrl_l_streaming_is_locked() {
        let (c, m) = ctrl('l');
        assert_eq!(classify(Streaming, true, c, m), Locked);
    }

    #[test]
    fn esc_idle_returns_back() {
        assert_eq!(classify(Idle, true, KeyCode::Esc, KeyModifiers::NONE), Back);
    }

    #[test]
    fn esc_streaming_returns_back() {
        assert_eq!(
            classify(Streaming, true, KeyCode::Esc, KeyModifiers::NONE),
            Back
        );
    }

    #[test]
    fn ctrl_q_idle_opens_quit_modal() {
        let (c, m) = ctrl('q');
        assert_eq!(classify(Idle, true, c, m), RequestQuit);
    }

    #[test]
    fn ctrl_q_streaming_opens_quit_modal() {
        let (c, m) = ctrl('q');
        assert_eq!(classify(Streaming, true, c, m), RequestQuit);
    }

    #[test]
    fn scroll_keys_work_in_every_state() {
        for state in [Idle, Streaming] {
            assert_eq!(
                classify(state, true, KeyCode::Up, KeyModifiers::NONE),
                ScrollUp
            );
            assert_eq!(
                classify(state, true, KeyCode::Down, KeyModifiers::NONE),
                ScrollDown
            );
        }
    }

    #[test]
    fn plain_char_idle_feeds_editor() {
        assert_eq!(
            classify(Idle, true, KeyCode::Char('h'), KeyModifiers::NONE),
            FeedEditor
        );
    }

    #[test]
    fn plain_p_without_control_feeds_editor() {
        assert_eq!(
            classify(Idle, true, KeyCode::Char('p'), KeyModifiers::NONE),
            FeedEditor
        );
    }

    #[test]
    fn pushup_prompt_contains_issue_number_and_skill() {
        let p = pushup_prompt(42);
        assert!(p.contains("/pushup"), "got: {p}");
        assert!(p.contains("#42"), "got: {p}");
    }
}
