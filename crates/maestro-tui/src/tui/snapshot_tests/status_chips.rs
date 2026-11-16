//! NO_COLOR snapshot of every status chip (#1251): each row must read
//! without colour, in both Unicode and ASCII icon modes.

use insta::assert_snapshot;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::no_color::no_color_snapshot;
use crate::tui::ds::status_chip::{ALL_STATUSES, status_chip_for_mode};

#[test]
fn status_chips_no_color() {
    let out = no_color_snapshot(|f, theme| {
        let rows: Vec<Line> = ALL_STATUSES
            .iter()
            .map(|&status| {
                let unicode = status_chip_for_mode(status, theme, true);
                let ascii = status_chip_for_mode(status, theme, false);
                let pad = 20usize.saturating_sub(unicode.width());
                Line::from(vec![unicode, Span::raw(" ".repeat(pad)), ascii])
            })
            .collect();
        f.render_widget(Paragraph::new(rows), f.area());
    });
    assert_snapshot!(out);
}
