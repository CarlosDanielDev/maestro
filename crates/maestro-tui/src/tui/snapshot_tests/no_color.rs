//! NO_COLOR snapshot harness (#1247): render any screen with the `NoColor`
//! theme and return the buffer *with its styles*, so the snapshot shows which
//! modifiers carry the meaning. It panics if any cell draws a colour other
//! than `Reset` (#1346), so a colour hard-coded outside the theme fails here.
//!
//! Add a NO_COLOR snapshot to a screen in one line:
//! `assert_snapshot!(no_color_snapshot(|f, theme| screen.draw(f, f.area(), theme)));`

use ratatui::Frame;
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};

use super::test_terminal;
use crate::tui::theme::Theme;

/// Draw once on the standard 120x40 test terminal with `Theme::no_color()`
/// and return the buffer's `Debug` output (content plus styles). Panics if
/// any cell's fg, bg or underline colour is not `Reset`.
pub fn no_color_snapshot(draw: impl FnOnce(&mut Frame, &Theme)) -> String {
    no_color_in(test_terminal(), draw)
}

/// [`no_color_snapshot`] on a `width` x `height` terminal, for screens
/// that lay out by breakpoint (Home Wide is 150+ columns, #1264).
pub fn no_color_snapshot_sized(
    width: u16,
    height: u16,
    draw: impl FnOnce(&mut Frame, &Theme),
) -> String {
    no_color_in(
        Terminal::new(TestBackend::new(width, height)).unwrap(),
        draw,
    )
}

fn no_color_in(
    mut terminal: Terminal<TestBackend>,
    draw: impl FnOnce(&mut Frame, &Theme),
) -> String {
    let theme = Theme::no_color();
    terminal
        .draw(|f| draw(f, &theme))
        .expect("draw must succeed");
    let buffer = terminal.backend().buffer();
    for (i, cell) in buffer.content.iter().enumerate() {
        let (x, y) = buffer.pos_of(i);
        for colour in [cell.fg, cell.bg, cell.underline_color] {
            assert_eq!(
                colour,
                Color::Reset,
                "NoColor drew a colour at ({x}, {y}): {cell:?}"
            );
        }
    }
    format!("{buffer:?}")
}

#[test]
#[should_panic(expected = "NoColor drew a colour")]
fn no_color_snapshot_fails_on_a_hard_coded_colour() {
    no_color_snapshot(|f, _theme| {
        let leak = ratatui::widgets::Paragraph::new("leak")
            .style(ratatui::style::Style::default().fg(ratatui::style::Color::Red));
        f.render_widget(leak, f.area());
    });
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
