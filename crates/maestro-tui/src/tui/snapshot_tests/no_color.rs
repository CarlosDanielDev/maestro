//! NO_COLOR snapshot harness (#1247): render any screen with the `NoColor`
//! theme and return the buffer *with its styles*, so the snapshot shows which
//! modifiers carry the meaning. It does not assert that no colour is drawn:
//! colours hard-coded outside the theme still show up in the snapshot. Known
//! exception: `agent_badge::agent_color` (see `docs/design-system/decisions.md`,
//! 2026-11-16 NoColor gap).
//!
//! Add a NO_COLOR snapshot to a screen in one line:
//! `assert_snapshot!(no_color_snapshot(|f, theme| screen.draw(f, f.area(), theme)));`

use ratatui::Frame;

use super::test_terminal;
use crate::tui::theme::Theme;

/// Draw once on the standard 120x40 test terminal with `Theme::no_color()`
/// and return the buffer's `Debug` output (content plus styles).
pub fn no_color_snapshot(draw: impl FnOnce(&mut Frame, &Theme)) -> String {
    let mut terminal = test_terminal();
    let theme = Theme::no_color();
    terminal
        .draw(|f| draw(f, &theme))
        .expect("draw must succeed");
    format!("{:?}", terminal.backend().buffer())
}

#[test]
fn no_color_snapshot_draws_with_no_color_theme() {
    let out = no_color_snapshot(|f, theme| {
        f.render_widget(theme.styled_block("Probe", true), f.area());
    });
    assert!(out.contains("[ Probe ]"), "block title must render: {out}");
    for colour in ["Green", "Yellow", "White", "Black", "Rgb", "Indexed"] {
        assert!(!out.contains(colour), "NoColor drew {colour}: {out}");
    }
}
