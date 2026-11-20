//! Footer of key hints (design system §04 `ds::key_hints`): one row at the
//! bottom of every screen with the hints for the focused context, and
//! the quit hint ([`QUIT`]) pinned right. Inside the quit dialog the pin is
//! left out.
//!
//! Hints are kept in order and the first one that does not fit ends the
//! row. They are measured by their drawn width, not with
//! `navigation::keymap::fit_hints_to_width`: that helper counts bytes and the
//! `[k] label` form, so it drops hints that fit (`↑/↓` is 7 bytes).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::bar_row::{draw_left_right, key_style};
use crate::tui::navigation::keymap::InlineHint;
use crate::tui::theme::Theme;

/// The pinned quit hint, `(key, label)`: ⌃Q quits from every screen, text
/// input included (#1260).
pub const QUIT: (&str, &str) = ("⌃Q", "quit");

/// Turn a screen's `(key, label)` list into hints, first = highest priority.
pub fn from_pairs(pairs: &[(&'static str, &'static str)]) -> Vec<InlineHint> {
    pairs
        .iter()
        .enumerate()
        .map(|(i, (key, action))| InlineHint {
            key,
            action,
            priority: i.min(u8::MAX as usize) as u8,
        })
        .collect()
}

pub fn draw(f: &mut Frame, area: Rect, hints: &[InlineHint], pin_quit: bool, theme: &Theme) {
    draw_dimmed(f, area, hints, &[], pin_quit, theme);
}

/// [`draw`], with the hints whose key is in `dimmed` drawn muted and DIM:
/// they are shown, but cannot act right now (#1399).
pub fn draw_dimmed(
    f: &mut Frame,
    area: Rect,
    hints: &[InlineHint],
    dimmed: &[&str],
    pin_quit: bool,
    theme: &Theme,
) {
    let key = key_style(theme);
    let label = Style::default().fg(theme.text_muted);
    let off = Style::default()
        .fg(theme.text_muted)
        .add_modifier(Modifier::DIM);
    let pair = |k: &str, l: &str| {
        let (k_style, l_style) = if dimmed.contains(&k) {
            (off, off)
        } else {
            (key, label)
        };
        [
            Span::styled(k.to_string(), k_style),
            Span::styled(format!(" {l}"), l_style),
        ]
    };
    let mut right = Line::default();
    if pin_quit {
        right.spans.extend(pair(QUIT.0, QUIT.1));
        right.spans.push(Span::raw(" "));
    }
    // Keep a gap of at least one column before the pin.
    let room = (area.width as usize).saturating_sub(right.width() + 1);
    let mut left = Line::from(" ");
    for (i, hint) in hints.iter().enumerate() {
        let sep = if i > 0 { "   " } else { "" };
        let next: Vec<Span> = [Span::raw(sep)]
            .into_iter()
            .chain(pair(hint.key, hint.action))
            .collect();
        let width: usize = next.iter().map(Span::width).sum();
        if left.width() + width > room {
            break;
        }
        left.spans.extend(next);
    }
    draw_left_right(f, area, left, right);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn quit_hint() -> String {
        format!("{} {}", QUIT.0, QUIT.1)
    }

    fn row(hints: &[InlineHint], pin_quit: bool, width: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(width, 1)).unwrap();
        let theme = Theme::dark();
        t.draw(|f| draw(f, f.area(), hints, pin_quit, &theme))
            .unwrap();
        let buf = t.backend().buffer();
        (0..width).map(|x| buf[(x, 0)].symbol()).collect()
    }

    #[test]
    fn hints_render_key_then_label() {
        let out = row(&from_pairs(&[("Enter", "open"), ("Esc", "back")]), true, 80);
        assert!(out.starts_with(" Enter open   Esc back"), "{out:?}");
    }

    #[test]
    fn hints_that_do_not_fit_are_dropped_from_the_end() {
        let hints = from_pairs(&[("↑/↓", "Field"), ("Esc", "Back"), ("x", "a long label")]);
        let out = row(&hints, true, 30);
        assert!(out.starts_with(" ↑/↓ Field   Esc Back"), "{out:?}");
        assert!(!out.contains("long"), "{out:?}");
        assert!(out.trim_end().ends_with(&quit_hint()), "{out:?}");
    }

    #[test]
    fn quit_dialog_drops_the_pin() {
        let out = row(&from_pairs(&[("y", "quit")]), false, 80);
        assert!(!out.contains(&quit_hint()), "{out:?}");
    }
}
