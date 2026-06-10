//! Scroll / paging state transitions for the Interaction transcript (#988).
//! Split out of `mod.rs` to keep that file under the 400-line cap. Pure state
//! mutation over `auto_scroll` / `scroll_offset`; the concrete offset is
//! recomputed at draw time by `layout::effective_offset`.

use super::InteractionScreen;

impl InteractionScreen {
    /// Scroll the history up by `n` lines. Takes manual control of the
    /// viewport (disables tail-following). `pub(crate)` so the mouse-wheel
    /// routing in `tui::mod` can drive it (#988).
    pub(crate) fn scroll_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll the history down by `n` lines, clamped to the last-known
    /// bottom. Re-pins tail-following once the bottom is reached. `pub(crate)`
    /// for the mouse-wheel routing (#988).
    pub(crate) fn scroll_down(&mut self, n: usize) {
        let max = self.last_max_offset;
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max);
        if self.scroll_offset >= max {
            self.auto_scroll = true;
        }
    }

    /// Page the transcript up by one viewport height (#988). Clamps at the top
    /// via `scroll_up`'s `saturating_sub`.
    pub(super) fn page_up(&mut self) {
        self.scroll_up(self.last_viewport.max(1));
    }

    /// Page the transcript down by one viewport height (#988). Clamps at
    /// `last_max_offset` and re-pins tail-following via `scroll_down`.
    pub(super) fn page_down(&mut self) {
        self.scroll_down(self.last_viewport.max(1));
    }

    /// Jump to the newest message and resume tail-following (#988). The draw
    /// path recomputes the concrete offset from `auto_scroll`.
    pub(super) fn jump_to_latest(&mut self) {
        self.auto_scroll = true;
        self.scroll_offset = self.last_max_offset;
    }
}
