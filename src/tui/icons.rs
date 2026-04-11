//! Icon mode detection: Nerd Font vs ASCII fallback.
//! Set `MAESTRO_ASCII_ICONS=1` to use ASCII-only icons.

use std::sync::OnceLock;

#[allow(dead_code)] // Reason: referenced by use_nerd_font() which is used in tests
static USE_NERD_FONT: OnceLock<bool> = OnceLock::new();

// Testable version: accepts an env-var reader closure.
#[allow(dead_code)] // Reason: used in tests
pub(crate) fn use_nerd_font_from_env(get_env: impl Fn(&str) -> Option<String>) -> bool {
    get_env("MAESTRO_ASCII_ICONS")
        .map(|v| v != "1")
        .unwrap_or(true)
}

/// Returns true if Nerd Font icons should be used (cached at first call).
#[allow(dead_code)] // Reason: public API for icon mode, used in tests
pub fn use_nerd_font() -> bool {
    *USE_NERD_FONT.get_or_init(|| use_nerd_font_from_env(|k| std::env::var(k).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionStatus;

    #[test]
    fn returns_true_when_env_var_absent() {
        assert!(use_nerd_font_from_env(|_| None));
    }

    #[test]
    fn returns_false_when_ascii_icons_set_to_1() {
        assert!(!use_nerd_font_from_env(|_| Some("1".to_string())));
    }

    #[test]
    fn returns_true_when_ascii_icons_set_to_0() {
        assert!(use_nerd_font_from_env(|_| Some("0".to_string())));
    }

    #[test]
    fn nerd_symbol_all_variants_are_nonempty() {
        use SessionStatus::*;
        let variants = [
            Queued, Spawning, Running, Completed, GatesRunning, NeedsReview,
            Errored, Paused, Killed, Stalled, Retrying, CiFix, NeedsPr, ConflictFix,
        ];
        for v in variants {
            assert!(!v.nerd_symbol().is_empty(), "{v:?}.nerd_symbol() is empty");
        }
    }

    #[test]
    fn ascii_symbol_all_variants_are_ascii_bracketed() {
        use SessionStatus::*;
        let variants = [
            Queued, Spawning, Running, Completed, GatesRunning, NeedsReview,
            Errored, Paused, Killed, Stalled, Retrying, CiFix, NeedsPr, ConflictFix,
        ];
        for v in variants {
            let s = v.ascii_symbol();
            assert!(s.is_ascii(), "{v:?}.ascii_symbol() returned non-ASCII: {s}");
            assert!(
                s.starts_with('[') && s.ends_with(']'),
                "{v:?}.ascii_symbol() must be [X] form, got: {s}"
            );
        }
    }

    #[test]
    fn nerd_and_ascii_symbols_are_distinct_for_running() {
        let nerd = SessionStatus::Running.nerd_symbol();
        let ascii = SessionStatus::Running.ascii_symbol();
        assert_ne!(nerd, ascii, "nerd and ascii symbols must be distinct for Running");
    }
}
