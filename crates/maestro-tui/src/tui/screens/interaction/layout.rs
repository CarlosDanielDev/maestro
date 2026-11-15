//! Pure layout/scroll math for the Interaction screen. Split out of mod.rs to
//! keep that file under the 400-line cap. No rendering, no state — just rects
//! and offsets, exhaustively unit-testable (tested in `tests.rs`).

use ratatui::layout::Rect;

/// Fixed height (rows, incl. borders) reserved for the input pane.
pub(super) const INPUT_HEIGHT: u16 = 5;

/// Header rows (top + bottom border of the square header box, #987 QA).
pub(super) const HEADER_HEIGHT: u16 = 2;

/// Shrink a rect by `margin` columns on each side, keeping it non-degenerate.
pub(super) fn inset_x(area: Rect, margin: u16) -> Rect {
    let trim = margin.saturating_mul(2);
    Rect {
        x: area.x.saturating_add(margin),
        y: area.y,
        width: area.width.saturating_sub(trim),
        height: area.height,
    }
}

/// Compute the vertical scroll offset to render at. When `auto_scroll` is
/// on, the pane follows the tail (returns the max offset). When off, it
/// honors the user's `scroll_offset`, clamped so it never scrolls past the
/// last line.
pub(super) fn effective_offset(
    auto_scroll: bool,
    scroll_offset: usize,
    total: usize,
    viewport: usize,
) -> usize {
    let max = total.saturating_sub(viewport);
    if auto_scroll {
        max
    } else {
        scroll_offset.min(max)
    }
}
