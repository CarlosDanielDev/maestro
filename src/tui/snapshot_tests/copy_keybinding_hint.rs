use crate::session::types::SessionStatus;
use crate::tui::app::TuiMode;
use crate::tui::keybinding_hints::keybinding_hints_spans;
use crate::tui::navigation::keymap::{fit_hints_to_width, mode_keymap};
use crate::tui::theme::Theme;
use insta::assert_debug_snapshot;
use ratatui::text::Span;

fn render_spans(copy_enabled: bool) -> Vec<Span<'static>> {
    let km = mode_keymap(TuiMode::Overview, Some(SessionStatus::Completed), &[]);
    let theme = Theme::dark();
    let fitted = fit_hints_to_width(km.hints, 120);
    keybinding_hints_spans(&fitted, copy_enabled, &theme)
}

#[test]
fn copy_keybinding_hint_enabled() {
    assert_debug_snapshot!(render_spans(true));
}

#[test]
fn copy_keybinding_hint_disabled() {
    assert_debug_snapshot!(render_spans(false));
}
