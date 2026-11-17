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

/// Initialize icon mode from config. Call at startup and on config reload.
/// `MAESTRO_ASCII_ICONS=1` overrides `ascii_icons = false`; the env var is
/// read here, once, so per-frame lookups stay a single atomic load.
pub fn init_from_config(ascii_icons: bool) {
    init_from_config_with_env(ascii_icons, process_env);
}

/// Testable version of `init_from_config`: accepts an env-var reader closure.
pub(crate) fn init_from_config_with_env(
    ascii_icons: bool,
    get_env: impl Fn(&str) -> Option<String>,
) {
    let ascii = ascii_icons || !use_nerd_font_from_env(get_env);
    let value = if ascii { ASCII } else { NERD_FONT };
    MODE.store(value, Ordering::Release);
}

/// Test-only: set the mode from the config flag alone, ignoring
/// `MAESTRO_ASCII_ICONS`, so tests stay hermetic when a developer has the
/// env var exported. Public so `maestro-tui` tests can reach it.
#[doc(hidden)]
pub fn init_ignoring_env(ascii_icons: bool) {
    init_from_config_with_env(ascii_icons, |_| None);
}

/// Returns true if Nerd Font icons should be used.
/// Reads the mode set by `init_from_config` (config flag or env override);
/// before init, falls back to the `MAESTRO_ASCII_ICONS` env var.
pub fn use_nerd_font() -> bool {
    match MODE.load(Ordering::Acquire) {
        NERD_FONT => true,
        ASCII => false,
        _ => use_nerd_font_from_env(process_env),
    }
}

/// The real process env, read by `init_from_config` and the uninitialized
/// arm of `use_nerd_font`.
#[cfg(not(any(test, feature = "test-util")))]
fn process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Test builds never read the real env, so the suite gives the same result
/// whether or not a developer has `MAESTRO_ASCII_ICONS` exported (#1354).
/// Tests that need the override inject it via `init_from_config_with_env`.
#[cfg(any(test, feature = "test-util"))]
fn process_env(_key: &str) -> Option<String> {
    None
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
    use std::sync::Mutex;

    // Tests that mutate the global MODE atomic must not run in parallel
    // with each other. Cargo runs #[test] functions concurrently by default,
    // which lets one test's init_from_config() land between another test's
    // write-and-assert pair and flip the answer — a deterministic-looking
    // failure that surfaces as a race flake in CI.
    //
    // Guarding the MODE-mutating tests with a shared mutex serializes them
    // without affecting tests that don't touch MODE (the env-var tests).
    static MODE_TEST_LOCK: Mutex<()> = Mutex::new(());

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
        let _guard = MODE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        init_from_config(true);
        assert!(!use_nerd_font());
        init_ignoring_env(false); // restore
    }

    #[test]
    fn init_from_config_ascii_false_enables_nerd_font() {
        let _guard = MODE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        init_ignoring_env(false);
        assert!(use_nerd_font());
    }

    #[test]
    fn env_var_overrides_config_ascii_false() {
        let _guard = MODE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        init_from_config_with_env(false, |_| Some("1".to_string()));
        assert!(!use_nerd_font());
        init_ignoring_env(false); // restore
    }

    #[test]
    fn lookups_keep_init_result_whatever_the_real_env() {
        // `use_nerd_font()` must return what init resolved, never re-read
        // the env per lookup. Both halves run, so a per-lookup env read
        // breaks one of them whether `MAESTRO_ASCII_ICONS` is set or not.
        let _guard = MODE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        init_from_config_with_env(false, |_| Some("1".to_string()));
        for _ in 0..1000 {
            assert!(!use_nerd_font());
        }
        init_from_config_with_env(false, |_| None);
        for _ in 0..1000 {
            assert!(use_nerd_font());
        }
    }

    #[test]
    fn use_nerd_font_reads_initialized_flag_not_env() {
        let _guard = MODE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        init_from_config(true);
        assert!(!use_nerd_font());
        init_ignoring_env(false);
        assert!(use_nerd_font());
    }
}
