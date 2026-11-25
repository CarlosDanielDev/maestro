//! Code health (`TuiMode::Sanitize`, §02.24) opened from the ⌃K palette
//! (#1276). The scan is the local `RustScanner` (no AI pass, no spend);
//! it runs in the background (`TuiCommand::ScanCodeHealth`) and its result
//! lands through `TuiDataEvent::CodeHealthScanned`.

use super::App;
use super::types::{TuiCommand, TuiMode};
use crate::sanitize::{SanitizeReport, ScanResult};
use crate::tui::screens::sanitize::SanitizeScreen;

/// What the screen says while the scan runs. The root is `.`, the same as
/// `maestro sanitize`.
pub(crate) const SCANNING: &str = "Scanning .rs files under . (local scan, no AI, free)…";

impl App {
    /// Open Code health on a "scanning" screen and start the scan.
    pub fn open_code_health(&mut self) {
        self.screen_state.sanitize_screen = Some(SanitizeScreen::with_status(SCANNING));
        self.navigate_to(TuiMode::Sanitize);
        self.pending_commands.push(TuiCommand::ScanCodeHealth);
    }

    /// The scan finished. Nothing happens if Code health was closed first.
    pub(crate) fn finish_code_health(&mut self, result: anyhow::Result<ScanResult>) {
        let Some(screen) = self.screen_state.sanitize_screen.as_mut() else {
            return;
        };
        *screen = match result {
            Ok(scan) => SanitizeScreen::new(SanitizeReport {
                scan,
                analysis: Default::default(),
            }),
            Err(e) => SanitizeScreen::with_status(crate::tui::screens::sanitize_for_terminal(
                &format!("Scan failed: {e}"),
            )),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sanitize::{Finding, Severity, SmellCategory, SourceLocation};

    fn finding() -> Finding {
        Finding {
            severity: Severity::Warning,
            category: SmellCategory::LongMethod,
            location: SourceLocation {
                file: "src/a.rs".into(),
                line_start: 1,
                line_end: 80,
            },
            message: "long".into(),
            dead_lines: 0,
        }
    }

    #[test]
    fn scan_result_fills_the_open_screen() {
        let mut app = crate::tui::make_test_app("code-health-result");
        app.open_code_health();
        app.finish_code_health(Ok(ScanResult {
            findings: vec![finding()],
        }));
        let screen = app.screen_state.sanitize_screen.as_ref().expect("open");
        assert_eq!(screen.finding_count(), 1);
        assert_eq!(screen.status, None);
    }

    #[test]
    fn scan_error_is_said_on_screen() {
        let mut app = crate::tui::make_test_app("code-health-error");
        app.open_code_health();
        app.finish_code_health(Err(anyhow::anyhow!("no such dir")));
        let screen = app.screen_state.sanitize_screen.as_ref().expect("open");
        assert_eq!(screen.status.as_deref(), Some("Scan failed: no such dir"));
    }

    #[test]
    fn result_after_close_does_nothing() {
        let mut app = crate::tui::make_test_app("code-health-closed");
        app.finish_code_health(Ok(ScanResult::default()));
        assert!(app.screen_state.sanitize_screen.is_none());
    }
}
