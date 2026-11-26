use crate::tui::keybinding_hints::keybinding_hints_spans;
use crate::tui::navigation::keymap::{InlineHint, fit_hints_to_width};
use crate::tui::theme::Theme;
use insta::assert_debug_snapshot;
use ratatui::text::Span;

/// The hint row of the old session grid (#1294 removed the mode); the
/// `c` Copy hint dims when there is nothing to copy.
const HINTS: &[InlineHint] = &[
    InlineHint {
        key: "Enter",
        action: "Detail",
        priority: 0,
    },
    InlineHint {
        key: "c",
        action: "Copy",
        priority: 1,
    },
    InlineHint {
        key: "d",
        action: "Log",
        priority: 2,
    },
    InlineHint {
        key: "f",
        action: "Full",
        priority: 3,
    },
    InlineHint {
        key: "w",
        action: "Switcher",
        priority: 4,
    },
    InlineHint {
        key: "Tab",
        action: "Cycle Views",
        priority: 5,
    },
];

fn render_spans(copy_enabled: bool) -> Vec<Span<'static>> {
    let theme = Theme::dark();
    let fitted = fit_hints_to_width(HINTS, 120);
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
