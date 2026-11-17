//! Transient toast shown after a Copy action (~2 s, see
//! [`crate::tui::app::clipboard_action::COPY_TOAST_TTL_MS`]). A thin
//! caller of [`crate::tui::ds::toast`] (#1254): success / error severity,
//! drawn at the bottom right of `area`.

use ratatui::{Frame, layout::Rect};

use crate::tui::ds::toast::{self, Severity, ToastView};
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy)]
pub enum CopyToastKind {
    Success,
    Error,
}

pub fn draw_copy_toast(
    f: &mut Frame,
    area: Rect,
    kind: CopyToastKind,
    message: &str,
    theme: &Theme,
) {
    let severity = match kind {
        CopyToastKind::Success => Severity::Success,
        CopyToastKind::Error => Severity::Error,
    };
    let view = ToastView {
        severity,
        message,
        undo_left: None,
    };
    toast::draw(f, area, &view, theme);
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn screen(kind: CopyToastKind, message: &str) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let theme = Theme::dark();
        terminal
            .draw(|f| draw_copy_toast(f, f.area(), kind, message, &theme))
            .expect("draw");
        terminal
    }

    #[test]
    fn clipboard_toast_success() {
        let t = screen(CopyToastKind::Success, "Response copied to clipboard");
        assert_snapshot!("clipboard_toast_success", t.backend());
    }

    #[test]
    fn clipboard_toast_error() {
        let t = screen(CopyToastKind::Error, "Copy failed: clipboard unavailable");
        assert_snapshot!("clipboard_toast_error", t.backend());
    }
}
