//! Minimal clipboard-read seam for the Launch / IssuePicker paste path.
//! Kept separate from the editor's richer `ClipboardProvider` (which carries
//! Image/Empty/Unavailable variants) because the issue picker only consumes
//! UTF-8 text — pulling in the richer enum would violate Demeter at the
//! call site.

pub trait IssueClipboard: Send {
    fn read(&self) -> Option<String>;
}

pub struct SystemIssueClipboard;

impl IssueClipboard for SystemIssueClipboard {
    fn read(&self) -> Option<String> {
        if !crate::tui::clipboard::backend_available() {
            return None;
        }
        let cb = std::panic::catch_unwind(arboard::Clipboard::new)
            .ok()?
            .ok()?;
        let mut cb = cb;
        let raw = cb.get_text().ok()?;
        // Strip control chars (defense-in-depth — `parse_pasted_issue_token`
        // already filters to digits, but raw clipboard bytes live briefly in
        // `launch.manual_issue_input` between paste and parse).
        let sanitized: String = raw
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect();
        let trimmed = sanitized.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
}

#[cfg(test)]
pub(super) mod testing {
    use super::IssueClipboard;
    use std::sync::Mutex;

    pub struct StubClipboard {
        inner: Mutex<Option<String>>,
    }

    impl StubClipboard {
        pub fn with_text(s: impl Into<String>) -> Self {
            Self {
                inner: Mutex::new(Some(s.into())),
            }
        }
        pub fn empty() -> Self {
            Self {
                inner: Mutex::new(None),
            }
        }
    }

    impl IssueClipboard for StubClipboard {
        fn read(&self) -> Option<String> {
            self.inner.lock().ok()?.clone()
        }
    }
}
