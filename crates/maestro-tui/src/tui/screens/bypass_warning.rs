//! Danger warning for bypass mode (#328).
//!
//! The user must type `CONFIRM`, then press `⏎`, every time they turn bypass
//! mode on from the TUI (#1276). `Esc` cancels (#1261).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

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
    /// The `⌃B` warning with no session started or queued.
    pub fn new() -> Self {
        Self::tui(0, 0)
    }

    /// The `⌃B` warning (§02.24): what bypass turns off (both effects, by
    /// flag), what stays, and when it turns off. `running` sessions keep
    /// their mode; `queued` ones are spawned with the pool's mode.
    pub fn tui(running: usize, queued: usize) -> Self {
        let row = |label: &str, text: &str| (label.to_string(), text.to_string());
        Self::with_text(
            "DANGER: enable bypass mode?",
            vec![
                row("", "What it turns off"),
                row("asks", "Claude's permission prompts (bypassPermissions)"),
                row("review", "your review step: every concern is auto-accepted"),
                row("", "What stays"),
                row("live", &format!("{} keep their mode", sessions(running))),
                row("queue", &format!("{} will get bypass", sessions(queued))),
                row("file", "maestro.toml is not written (Settings writes it)"),
                row("trust", "workspace trust still applies"),
                row("", "Turns off when"),
                row("off", "Ctrl+B, a Settings save, or the end of a PR review"),
            ],
        )
    }

    /// The same `CONFIRM` dialog with another title and consequences, for a
    /// caller whose scope differs from `⌃B` (e.g. a write to `maestro.toml`).
    pub fn with_text(title: impl Into<String>, consequences: Vec<(String, String)>) -> Self {
        Self {
            confirm: TypedConfirm::new(title, CONFIRM_PHRASE, "enable", consequences),
        }
    }
}

/// `1 session` / `2 sessions`.
fn sessions(n: usize) -> String {
    if n == 1 {
        "1 session".to_string()
    } else {
        format!("{n} sessions")
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
    fn with_text_keeps_the_confirm_phrase() {
        let mut state = BypassWarningState::with_text(
            "Turn on bypass for new sessions?",
            vec![("scope".into(), "maestro.toml is committed".into())],
        );
        assert_eq!(state.confirm.expected(), "CONFIRM");
        type_str(&mut state, "CONFIRM");
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Enter)),
            BypassWarningOutcome::Confirmed
        );
    }

    /// #1276: the dialog names both effects and counts the sessions.
    #[test]
    fn tui_text_names_both_effects_and_counts() {
        let state = BypassWarningState::tui(2, 1);
        let mut t =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("terminal");
        let theme = crate::tui::theme::Theme::dark();
        t.draw(|f| state.confirm.render(f, f.area(), &theme))
            .expect("draw");
        let text = format!("{}", t.backend());
        for want in [
            "bypassPermissions",
            "auto-accepted",
            "2 sessions keep their mode",
            "1 session will get bypass",
            "maestro.toml is not written",
            "Turns off when",
        ] {
            assert!(text.contains(want), "missing {want:?} in:\n{text}");
        }
    }

    #[test]
    fn token_is_the_confirm_phrase() {
        assert_eq!(BypassWarningState::new().confirm.expected(), "CONFIRM");
    }
}
