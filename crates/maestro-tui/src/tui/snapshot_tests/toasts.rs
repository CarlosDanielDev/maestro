//! Toast screen snapshots (#1254): one per severity, the undo countdown,
//! and a NO_COLOR pass where the glyph + word carry the meaning.

use std::time::Duration;

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::no_color::no_color_snapshot;
use crate::tui::ds::toast::{Severity, ToastView, draw_for_mode};
use crate::tui::theme::Theme;
use ratatui::layout::Rect;

fn screen(view: &ToastView<'_>) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    let theme = Theme::dark();
    terminal
        .draw(|f| draw_for_mode(f, f.area(), view, &theme, true))
        .expect("draw");
    terminal
}

fn view(severity: Severity, message: &str) -> ToastView<'_> {
    ToastView {
        severity,
        message,
        undo_left: None,
    }
}

#[test]
fn toast_info() {
    let t = screen(&view(Severity::Info, "Chat #1185 keeps working"));
    assert_snapshot!("toast_info", t.backend());
}

#[test]
fn toast_warn() {
    let t = screen(&view(Severity::Warn, "Context 70% on #409 · auto-forked"));
    assert_snapshot!("toast_warn", t.backend());
}

#[test]
fn toast_error() {
    let t = screen(&view(
        Severity::Error,
        "gh auth expired · run gh auth login",
    ));
    assert_snapshot!("toast_error", t.backend());
}

#[test]
fn toast_undo_countdown() {
    let t = screen(&ToastView {
        severity: Severity::Success,
        message: "Issue #418 created",
        undo_left: Some(Duration::from_millis(3_200)),
    });
    assert_snapshot!("toast_undo_countdown", t.backend());
}

#[test]
fn toast_no_color() {
    let out = no_color_snapshot(|f, theme| {
        for (i, severity) in [
            Severity::Success,
            Severity::Info,
            Severity::Warn,
            Severity::Error,
        ]
        .into_iter()
        .enumerate()
        {
            let area = Rect::new(0, 0, 80, 5 + 4 * i as u16);
            draw_for_mode(f, area, &view(severity, "probe"), theme, true);
        }
    });
    for word in ["ok", "info", "warn", "error"] {
        assert!(out.contains(word), "severity word {word} must render");
    }
    assert_snapshot!("toast_no_color", out);
}

#[test]
fn toast_ascii_glyphs() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    let theme = Theme::dark();
    terminal
        .draw(|f| draw_for_mode(f, f.area(), &view(Severity::Error, "x"), &theme, false))
        .expect("draw");
    let text = format!("{:?}", terminal.backend().buffer());
    assert!(text.contains("[X] error"), "ASCII glyph + word: {text}");
}

#[test]
fn toast_strips_control_chars() {
    let t = screen(&view(Severity::Error, "bad\u{1b}[31mred"));
    let text = format!("{:?}", t.backend().buffer());
    assert!(!text.contains('\u{1b}'), "escape must be stripped");
}

#[test]
fn toast_skips_tiny_area() {
    let mut terminal = Terminal::new(TestBackend::new(8, 3)).expect("terminal");
    let theme = Theme::dark();
    terminal
        .draw(|f| draw_for_mode(f, f.area(), &view(Severity::Info, "hi"), &theme, true))
        .expect("draw must not panic");
}

#[test]
fn toast_countdown_stays_inside_narrow_toast() {
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).expect("terminal");
    let theme = Theme::dark();
    let undo = ToastView {
        severity: Severity::Success,
        message: "Issue #418 created",
        undo_left: Some(Duration::from_secs(5)),
    };
    terminal
        .draw(|f| draw_for_mode(f, f.area(), &undo, &theme, true))
        .expect("draw");
    let buf = terminal.backend().buffer();
    // Toast spans x 1..=18, rows 2..=4; the content row keeps its border.
    assert_eq!(buf[(18, 3)].symbol(), "│");
}
