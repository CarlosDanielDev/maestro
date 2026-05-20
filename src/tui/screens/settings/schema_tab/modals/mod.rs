//! Modal dialogs for dynamic-map / dynamic-rows widget primitives.
//!
//! Each modal owns its own state machine and exposes a `handle_input` that
//! returns a [`ModalAction`]. The hosting widget drives the lifecycle:
//! open, forward input, close on `Cancel`, commit on `Submit`.

use ratatui::layout::Rect;

pub mod add_entry;
pub mod remove_confirm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalAction {
    None,
    Cancel,
    Submit { id: String },
}

pub(crate) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
