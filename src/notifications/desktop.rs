//! Desktop notification dispatch seam.
//!
//! Production code holds an `Arc<dyn DesktopNotifier>`; tests inject
//! `FakeNotifier`. Real macOS dispatch goes through `OsascriptNotifier`,
//! which spawns a fire-and-forget `osascript` subprocess so the tokio
//! reactor never blocks. Errors are buffered and drained per render tick
//! by the TUI so permission failures land in the activity log instead of
//! disappearing silently.

use crate::util::formatting::truncate_with_ellipsis;
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub(crate) enum NotifyError {
    /// macOS user has not granted notification permission to the launcher
    /// (Terminal.app / iTerm.app / etc.).
    PermissionDenied,
    DispatchFailed(String),
    Internal(String),
}

impl fmt::Display for NotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotifyError::PermissionDenied => {
                f.write_str("permission denied — grant Notifications in System Settings")
            }
            NotifyError::DispatchFailed(msg) => write!(f, "dispatch failed: {}", msg),
            NotifyError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for NotifyError {}

/// Sends OS-level desktop notifications. Implementations MUST NOT block
/// the tokio reactor; if a backend is sync, it should `spawn_blocking`
/// internally. The `notify` method is fire-and-forget — errors are
/// buffered and drained via `take_last_error`.
pub(crate) trait DesktopNotifier: Send + Sync {
    /// Fire a notification. Disabled-toggle short-circuit lives inside
    /// the impl — callers do not branch on enablement.
    fn notify(&self, title: &str, body: &str);

    /// Returns and clears the most recent error, if any. Polled by the
    /// TUI render tick to surface permission failures.
    fn take_last_error(&self) -> Option<NotifyError>;
}

const NOTIFICATION_TITLE_MAX_BYTES: usize = 128;
const NOTIFICATION_BODY_MAX_BYTES: usize = 256;
/// Pinned to defeat PATH hijacking (CWE-426).
const OSASCRIPT_PATH: &str = "/usr/bin/osascript";

/// Escape control characters and string-literal delimiters for safe
/// interpolation into an AppleScript double-quoted string literal.
///
/// AppleScript terminates a `"..."` literal at a raw `\n` or `\r`, so
/// passing those through would let attacker-influenceable input (CLI
/// error messages, session labels) escape the string and execute as
/// arbitrary script.
pub(crate) fn sanitize_applescript(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// Build the `display notification` AppleScript for a title/body pair.
///
/// Regression seam for issue #1035: every byte of `title`/`body` must pass
/// through [`sanitize_applescript`] and the notification length caps, so no
/// raw newline, CR or unescaped quote from session-derived text can
/// terminate the string literal and execute as AppleScript.
fn build_notification_script(title: &str, body: &str) -> String {
    let safe_title =
        sanitize_applescript(&truncate_with_ellipsis(title, NOTIFICATION_TITLE_MAX_BYTES));
    let safe_body =
        sanitize_applescript(&truncate_with_ellipsis(body, NOTIFICATION_BODY_MAX_BYTES));
    format!(
        r#"display notification "{body}" with title "Maestro" subtitle "{title}""#,
        body = safe_body,
        title = safe_title,
    )
}

pub(crate) struct OsascriptNotifier {
    enabled: bool,
    last_error: Arc<Mutex<Option<NotifyError>>>,
}

impl OsascriptNotifier {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_error: Arc::new(Mutex::new(None)),
        }
    }
}

impl DesktopNotifier for OsascriptNotifier {
    fn notify(&self, title: &str, body: &str) {
        if !self.enabled {
            return;
        }

        let script = build_notification_script(title, body);
        let last_error = Arc::clone(&self.last_error);

        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                std::process::Command::new(OSASCRIPT_PATH)
                    .args(["-e", &script])
                    .output()
            })
            .await;

            let err = match result {
                Ok(Ok(out)) if out.status.success() => return,
                Ok(Ok(out)) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if stderr.contains("not authorized") || stderr.contains("-1743") {
                        NotifyError::PermissionDenied
                    } else {
                        NotifyError::DispatchFailed(stderr.into_owned())
                    }
                }
                Ok(Err(io)) => NotifyError::Internal(format!("spawn failed: {}", io)),
                Err(join) => NotifyError::Internal(format!("join failed: {}", join)),
            };

            tracing::warn!(error = %err, "desktop notify failed");
            let mut slot = last_error.lock().unwrap_or_else(|e| e.into_inner());
            *slot = Some(err);
        });
    }

    fn take_last_error(&self) -> Option<NotifyError> {
        self.last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NotificationCall {
    pub title: String,
    pub body: String,
}

#[cfg(test)]
#[derive(Debug)]
struct FakeNotifierInner {
    enabled: bool,
    calls: Mutex<Vec<NotificationCall>>,
    last_error: Mutex<Option<NotifyError>>,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct FakeNotifier {
    inner: Arc<FakeNotifierInner>,
}

#[cfg(test)]
impl FakeNotifier {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            inner: Arc::new(FakeNotifierInner {
                enabled,
                calls: Mutex::new(Vec::new()),
                last_error: Mutex::new(None),
            }),
        }
    }

    pub(crate) fn inject_error(&self, err: NotifyError) {
        let mut slot = self
            .inner
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *slot = Some(err);
    }

    pub(crate) fn calls(&self) -> Vec<NotificationCall> {
        self.inner
            .calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn call_count(&self) -> usize {
        self.inner
            .calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

#[cfg(test)]
impl DesktopNotifier for FakeNotifier {
    fn notify(&self, title: &str, body: &str) {
        if !self.inner.enabled {
            return;
        }
        let mut guard = self.inner.calls.lock().unwrap_or_else(|e| e.into_inner());
        guard.push(NotificationCall {
            title: title.to_owned(),
            body: body.to_owned(),
        });
    }

    fn take_last_error(&self) -> Option<NotifyError> {
        self.inner
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_notifier_records_call_when_enabled() {
        let fake = FakeNotifier::new(true);

        fake.notify("Session complete: #42", "Finished in 3.2s");

        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].title, "Session complete: #42");
        assert_eq!(calls[0].body, "Finished in 3.2s");
    }

    #[test]
    fn fake_notifier_is_noop_when_disabled() {
        let fake = FakeNotifier::new(false);

        fake.notify("Session complete: #7", "done");
        fake.notify("Session complete: #7", "done again");

        assert_eq!(fake.call_count(), 0);
    }

    #[test]
    fn osascript_notifier_is_noop_when_disabled() {
        let notifier = OsascriptNotifier::new(false);

        notifier.notify("should-not-fire", "body");

        assert!(notifier.take_last_error().is_none());
    }

    #[test]
    fn sanitize_applescript_escapes_double_quote_and_backslash() {
        let input = r#"a"b\c"#;

        let out = sanitize_applescript(input);

        assert_eq!(out, r#"a\"b\\c"#);
    }

    #[test]
    fn sanitize_applescript_escapes_newlines_and_carriage_returns() {
        // Without escaping, a raw newline terminates the AppleScript
        // string literal and lets the rest of the input parse as code.
        let input = "ok\nmalicious\rstuff";

        let out = sanitize_applescript(input);

        assert!(!out.contains('\n'), "raw LF must be escaped: {:?}", out);
        assert!(!out.contains('\r'), "raw CR must be escaped: {:?}", out);
        assert_eq!(out, r"ok\nmalicious\rstuff");
    }

    #[test]
    fn build_notification_script_escapes_newline_and_quote_payload() {
        // Issue #1035 repro: `"` followed by a raw newline would close the
        // string literal and let the rest of the message parse as
        // AppleScript code. The built script must carry no raw newline and
        // only escaped content quotes.
        let script = build_notification_script("t", "ok\"\ndisplay dialog \"pwn\"\n\"");

        assert!(
            !script.contains('\n'),
            "raw LF must be escaped: {:?}",
            script
        );
        assert!(
            !script.contains('\r'),
            "raw CR must be escaped: {:?}",
            script
        );
        assert_eq!(
            script,
            r#"display notification "ok\"\ndisplay dialog \"pwn\"\n\"" with title "Maestro" subtitle "t""#
        );
    }

    #[test]
    fn sanitize_applescript_drops_control_characters_and_nul() {
        let input = "a\u{0000}b\u{0007}c\u{001b}d";

        let out = sanitize_applescript(input);

        assert_eq!(out, "abcd");
    }

    #[test]
    fn take_last_error_returns_some_then_drain_returns_none() {
        let fake = FakeNotifier::new(true);
        fake.inject_error(NotifyError::PermissionDenied);

        let first = fake.take_last_error();
        assert!(
            matches!(first, Some(NotifyError::PermissionDenied)),
            "expected PermissionDenied, got {:?}",
            first
        );

        let second = fake.take_last_error();
        assert!(
            second.is_none(),
            "expected None after drain, got {:?}",
            second
        );
    }
}
