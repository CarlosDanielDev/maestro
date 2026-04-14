//! Shared icon mode detection — single source of truth for ASCII vs Nerd Font.
//!
//! This module lives in the lib crate so both `session::types` and `tui::icons`
//! can read the same atomic without duplication.
//!
//! A single `AtomicU8` encodes both "initialized?" and "which mode?" to avoid
//! split-state races between two separate atomics.

use std::sync::atomic::{AtomicU8, Ordering};

/// 0 = not yet initialized (fall back to env var).
const UNINITIALIZED: u8 = 0;
/// 1 = initialized, Nerd Font mode (ascii_icons = false).
const NERD_FONT: u8 = 1;
/// 2 = initialized, ASCII mode (ascii_icons = true).
const ASCII: u8 = 2;

static MODE: AtomicU8 = AtomicU8::new(UNINITIALIZED);

/// Initialize icon mode from config. Call once at startup.
pub fn init_from_config(ascii_icons: bool) {
    let value = if ascii_icons { ASCII } else { NERD_FONT };
    MODE.store(value, Ordering::Release);
}

/// Returns true if Nerd Font icons should be used.
/// Checks (in order): config flag, `MAESTRO_ASCII_ICONS` env var.
pub fn use_nerd_font() -> bool {
    match MODE.load(Ordering::Acquire) {
        NERD_FONT => true,
        ASCII => false,
        _ => use_nerd_font_from_env(|k| std::env::var(k).ok()),
    }
}

/// Testable version: accepts an env-var reader closure.
pub(crate) fn use_nerd_font_from_env(get_env: impl Fn(&str) -> Option<String>) -> bool {
    get_env("MAESTRO_ASCII_ICONS")
        .map(|v| v != "1")
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_absent_returns_nerd_font_true() {
        assert!(use_nerd_font_from_env(|_| None));
    }

    #[test]
    fn env_var_set_to_1_returns_nerd_font_false() {
        assert!(!use_nerd_font_from_env(|_| Some("1".to_string())));
    }

    #[test]
    fn env_var_set_to_0_returns_nerd_font_true() {
        assert!(use_nerd_font_from_env(|_| Some("0".to_string())));
    }

    #[test]
    fn env_var_arbitrary_value_returns_nerd_font_true() {
        assert!(use_nerd_font_from_env(|_| Some("yes".to_string())));
    }

    #[test]
    fn init_from_config_ascii_true_disables_nerd_font() {
        init_from_config(true);
        assert!(!use_nerd_font());
        init_from_config(false); // restore
    }

    #[test]
    fn init_from_config_ascii_false_enables_nerd_font() {
        init_from_config(false);
        assert!(use_nerd_font());
    }

    #[test]
    fn use_nerd_font_reads_initialized_flag_not_env() {
        init_from_config(true);
        assert!(!use_nerd_font());
        init_from_config(false);
        assert!(use_nerd_font());
    }
}
