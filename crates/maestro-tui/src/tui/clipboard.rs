#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

//! Write side of the system clipboard for the TUI.
//!
//! The read side (text + image fallback for paste) lives in
//! `screens::prompt_input::ClipboardProvider` and predates this module.
//! Both adapters wrap `arboard`. They are intentionally kept separate
//! for now; a follow-up may unify them under a single trait.

use anyhow::{Context, Result};
use std::sync::OnceLock;

/// Place plain UTF-8 text on the system clipboard.
pub trait Clipboard: Send + Sync {
    fn write(&self, text: &str) -> Result<()>;
}

static CLIPBOARD_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Returns true if `arboard::Clipboard::new()` produced a usable handle
/// on first call. Cached for the process lifetime so headless / WSL
/// environments fail fast on subsequent attempts without re-paying the
/// init cost or risking another panic. Shared between this module's
/// `SystemClipboard::write` and `screens::prompt_input::SystemClipboard::read`.
pub fn backend_available() -> bool {
    *CLIPBOARD_AVAILABLE.get_or_init(|| {
        std::panic::catch_unwind(arboard::Clipboard::new)
            .ok()
            .and_then(|r| r.ok())
            .is_some()
    })
}

pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn write(&self, text: &str) -> Result<()> {
        if !backend_available() {
            anyhow::bail!("clipboard unavailable");
        }
        // arboard can panic on Wayland/X11 connection loss even after the
        // initial probe succeeded; a panic on the input thread would
        // crash the TUI without restoring raw mode.
        let cb_result = std::panic::catch_unwind(arboard::Clipboard::new)
            .map_err(|_| anyhow::anyhow!("clipboard unavailable"))?;
        let mut cb = cb_result.context("clipboard unavailable")?;
        cb.set_text(text.to_owned())
            .context("clipboard unavailable")?;
        Ok(())
    }
}

/// Strip ANSI escape sequences (CSI, SGR, OSC, …) from `input` and any
/// residual C0 control bytes (NUL, BEL, BS, DEL, …) that the underlying
/// parser may pass through. LF (`\n`) is preserved; the underlying
/// `strip-ansi-escapes` (vte) also drops TAB and CR as part of its
/// terminal-state-machine handling. The post-filter is defence in
/// depth: if the parser ever leaves a control byte through, it doesn't
/// reach the OS clipboard (some platforms truncate at NUL).
pub fn strip_ansi(input: &str) -> String {
    strip_ansi_escapes::strip_str(input)
        .chars()
        .filter(|c| {
            let code = *c as u32;
            // Drop C0 (0x00..=0x1F) and DEL (0x7F), except 0x0A LF.
            !matches!(code, 0x00..=0x09 | 0x0B..=0x1F | 0x7F)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod testing {
    //! Test fakes shared across `clipboard` and `app::clipboard_action` tests.
    use super::Clipboard;
    use std::sync::{Arc, Mutex, MutexGuard};

    fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    struct MockState {
        writes: Mutex<Vec<String>>,
        fail_with: Mutex<Option<String>>,
    }

    /// Cloneable fake clipboard. The handle held by the test and the one
    /// passed to `App::with_clipboard(Box::new(...))` share the same
    /// inner state via `Arc`, so the test can read `recorded_writes()`
    /// after the production code wrote to it.
    #[derive(Clone)]
    pub(crate) struct MockClipboard {
        inner: Arc<MockState>,
    }

    impl MockClipboard {
        pub(crate) fn new() -> Self {
            Self {
                inner: Arc::new(MockState {
                    writes: Mutex::new(Vec::new()),
                    fail_with: Mutex::new(None),
                }),
            }
        }

        pub(crate) fn will_fail(msg: impl Into<String>) -> Self {
            Self {
                inner: Arc::new(MockState {
                    writes: Mutex::new(Vec::new()),
                    fail_with: Mutex::new(Some(msg.into())),
                }),
            }
        }

        pub(crate) fn recorded_writes(&self) -> Vec<String> {
            guard(&self.inner.writes).clone()
        }

        pub(crate) fn write_count(&self) -> usize {
            guard(&self.inner.writes).len()
        }
    }

    impl Clipboard for MockClipboard {
        fn write(&self, text: &str) -> anyhow::Result<()> {
            let fail = guard(&self.inner.fail_with).clone();
            if let Some(msg) = fail {
                return Err(anyhow::anyhow!("{}", msg));
            }
            guard(&self.inner.writes).push(text.to_owned());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_sgr_color_sequence() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m"), "hello");
    }

    #[test]
    fn strip_ansi_plain_text_unchanged() {
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn strip_ansi_empty_string_returns_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_multiple_sequences_all_removed() {
        let input = "\x1b[1;34mBold Blue\x1b[0m \x1b[41mRed BG\x1b[0m";
        assert_eq!(strip_ansi(input), "Bold Blue Red BG");
    }

    #[test]
    fn strip_ansi_cursor_movement_csi_removed() {
        let input = "\x1b[3Atext";
        assert_eq!(strip_ansi(input), "text");
    }

    #[test]
    fn strip_ansi_does_not_panic_on_partial_escape() {
        // The exact handling of an ESC byte that is not followed by a
        // recognized terminator depends on the parser's state machine;
        // the crate may consume trailing bytes. The contract we require
        // is purely "does not panic and returns valid UTF-8".
        let _ = strip_ansi("\x1babc");
    }

    #[test]
    fn strip_ansi_drops_c0_controls_but_keeps_lf() {
        let stripped = strip_ansi("a\x00b\x07c\x08d\x7fe\tf\ng\rh");
        assert!(!stripped.contains('\x00'), "NUL must be stripped");
        assert!(!stripped.contains('\x07'), "BEL must be stripped");
        assert!(!stripped.contains('\x08'), "BS must be stripped");
        assert!(!stripped.contains('\x7f'), "DEL must be stripped");
        assert!(stripped.contains('\n'), "LF must survive");
        assert!(stripped.contains("abc"));
    }
}
