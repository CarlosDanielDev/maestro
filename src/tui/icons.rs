//! Icon mode detection: Nerd Font vs ASCII fallback.
//! Config: `tui.ascii_icons = true` in maestro.toml
//! Env override: `MAESTRO_ASCII_ICONS=1`

use std::sync::atomic::{AtomicBool, Ordering};

static ASCII_MODE: AtomicBool = AtomicBool::new(false);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize icon mode from config. Call once at startup.
pub fn init_from_config(ascii_icons: bool) {
    ASCII_MODE.store(ascii_icons, Ordering::Relaxed);
    INITIALIZED.store(true, Ordering::Relaxed);
}

/// Returns true if Nerd Font icons should be used.
/// Checks (in order): config flag, MAESTRO_ASCII_ICONS env var.
#[allow(dead_code)] // Reason: public API used in tests and available for non-SessionStatus callers
pub fn use_nerd_font() -> bool {
    if INITIALIZED.load(Ordering::Relaxed) {
        return !ASCII_MODE.load(Ordering::Relaxed);
    }
    // Fallback: env var check (for lib crate / before config loads)
    use_nerd_font_from_env(|k| std::env::var(k).ok())
}

// Testable version: accepts an env-var reader closure.
#[allow(dead_code)] // Reason: used in tests
pub(crate) fn use_nerd_font_from_env(get_env: impl Fn(&str) -> Option<String>) -> bool {
    get_env("MAESTRO_ASCII_ICONS")
        .map(|v| v != "1")
        .unwrap_or(true)
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
    fn init_from_config_sets_ascii_mode() {
        init_from_config(true);
        assert!(!use_nerd_font());
        // Reset for other tests
        init_from_config(false);
    }

    #[test]
    fn nerd_symbol_all_variants_are_nonempty() {
        use SessionStatus::*;
        let variants = [
            Queued,
            Spawning,
            Running,
            Completed,
            GatesRunning,
            NeedsReview,
            Errored,
            Paused,
            Killed,
            Stalled,
            Retrying,
            CiFix,
            NeedsPr,
            ConflictFix,
        ];
        for v in variants {
            assert!(!v.nerd_symbol().is_empty(), "{v:?}.nerd_symbol() is empty");
        }
    }

    #[test]
    fn ascii_symbol_all_variants_are_ascii_bracketed() {
        use SessionStatus::*;
        let variants = [
            Queued,
            Spawning,
            Running,
            Completed,
            GatesRunning,
            NeedsReview,
            Errored,
            Paused,
            Killed,
            Stalled,
            Retrying,
            CiFix,
            NeedsPr,
            ConflictFix,
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
        assert_ne!(
            nerd, ascii,
            "nerd and ascii symbols must be distinct for Running"
        );
    }
}
