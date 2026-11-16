//! Width classes for layouts (design system §03 "Layout grid & breakpoints").
//!
//! `TooSmall` means the area is below [`MIN_SIZE`]: draw the SizeGuard
//! (`ds::size_guard`) instead of a layout that would overlap.

use ratatui::layout::Rect;

/// Smallest terminal maestro lays out, as `(columns, rows)`.
pub const MIN_SIZE: (u16, u16) = (80, 24);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    /// Below 80x24: draw the SizeGuard screen.
    TooSmall,
    /// 80-99 columns.
    Compact,
    /// 100-149 columns.
    Regular,
    /// 150 columns and wider.
    Wide,
}

impl Breakpoint {
    pub fn from_area(area: Rect) -> Self {
        if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
            return Self::TooSmall;
        }
        match area.width {
            ..=99 => Self::Compact,
            100..=149 => Self::Regular,
            _ => Self::Wide,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoint_edges() {
        let cases = [
            (79, Breakpoint::TooSmall),
            (80, Breakpoint::Compact),
            (99, Breakpoint::Compact),
            (100, Breakpoint::Regular),
            (149, Breakpoint::Regular),
            (150, Breakpoint::Wide),
        ];
        for (width, expected) in cases {
            let area = Rect::new(0, 0, width, MIN_SIZE.1);
            assert_eq!(Breakpoint::from_area(area), expected, "width {width}");
        }
    }

    #[test]
    fn breakpoint_short_terminal_is_too_small_at_any_width() {
        for width in [80, 120, 200] {
            let area = Rect::new(0, 0, width, MIN_SIZE.1 - 1);
            assert_eq!(Breakpoint::from_area(area), Breakpoint::TooSmall);
        }
    }
}
