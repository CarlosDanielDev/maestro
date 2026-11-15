//! UI state for the call-log viewer (#868).

/// Ephemeral state for one call-log pane: cursor, expand toggle, scroll
/// offsets for the list and the payload sub-panel. Lives on `App` because
/// `TuiMode::CallLog(Uuid)` is a peer top-level mode (mirrors
/// `App.log_viewer_scroll` for the existing free-text log viewer).
#[derive(Debug, Default, Clone)]
pub struct CallLogState {
    pub selected: usize,
    pub expanded: bool,
    pub list_scroll: u16,
    pub payload_scroll: u16,
    /// Live-tail mode: when on, the cursor auto-advances to the newest entry
    /// as the log grows. Toggled with `[f]`; disabled by any manual move (#886).
    pub follow_tail: bool,
    /// Entry count observed on the previous render tick, used to detect growth
    /// so follow-tail only snaps when new entries actually landed (#886).
    pub last_seen_total: usize,
}

impl CallLogState {
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self, total: usize) {
        if total == 0 {
            self.selected = 0;
            return;
        }
        let last = total.saturating_sub(1);
        if self.selected < last {
            self.selected += 1;
        }
    }

    pub fn jump_first(&mut self) {
        self.selected = 0;
    }

    pub fn jump_last(&mut self, total: usize) {
        self.selected = total.saturating_sub(1);
    }

    pub fn toggle_expand(&mut self) {
        self.expanded = !self.expanded;
        if !self.expanded {
            self.payload_scroll = 0;
        }
    }

    pub fn scroll_payload_up(&mut self) {
        self.payload_scroll = self.payload_scroll.saturating_sub(1);
    }

    pub fn scroll_payload_down(&mut self) {
        self.payload_scroll = self.payload_scroll.saturating_add(1);
    }

    /// Toggle live-tail follow mode.
    pub fn toggle_follow_tail(&mut self) {
        self.follow_tail = !self.follow_tail;
    }

    /// Turn off follow mode. Called when the user moves the cursor manually so
    /// they can scroll back without the auto-advance fighting them (#886).
    pub fn disable_follow_tail(&mut self) {
        self.follow_tail = false;
    }

    /// When follow mode is on and the entry count grew since the previous
    /// tick, snap the cursor to the newest entry. Called once per render tick
    /// (the event loop) before drawing. No-op when follow mode is off.
    pub fn reconcile_follow_tail(&mut self, total: usize) {
        if self.follow_tail && total > self.last_seen_total {
            self.selected = total.saturating_sub(1);
        }
        self.last_seen_total = total;
    }

    /// Clamp `selected` so it never points past the end of the log. Called
    /// by the renderer before drawing so the cursor survives a drain that
    /// happened while the pane was open.
    pub fn clamp_to_total(&mut self, total: usize) {
        if total == 0 {
            self.selected = 0;
            return;
        }
        if self.selected >= total {
            self.selected = total - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_at(selected: usize) -> CallLogState {
        CallLogState {
            selected,
            ..CallLogState::default()
        }
    }

    #[test]
    fn move_up_from_middle_decrements_selected() {
        let mut s = state_at(3);
        s.move_up();
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn move_up_from_zero_saturates_at_zero() {
        let mut s = state_at(0);
        s.move_up();
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn move_down_from_middle_increments_selected() {
        let mut s = state_at(2);
        s.move_down(10);
        assert_eq!(s.selected, 3);
    }

    #[test]
    fn move_down_at_last_item_clamps_to_last() {
        let mut s = state_at(9);
        s.move_down(10);
        assert_eq!(s.selected, 9);
    }

    #[test]
    fn move_down_with_zero_total_does_not_panic() {
        let mut s = CallLogState::default();
        s.move_down(0);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn jump_first_sets_selected_to_zero() {
        let mut s = state_at(7);
        s.jump_first();
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn jump_last_sets_selected_to_total_minus_one() {
        let mut s = state_at(0);
        s.jump_last(15);
        assert_eq!(s.selected, 14);
    }

    #[test]
    fn jump_last_with_zero_total_stays_at_zero() {
        let mut s = CallLogState::default();
        s.jump_last(0);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn toggle_expand_flips_false_to_true() {
        let mut s = CallLogState::default();
        s.toggle_expand();
        assert!(s.expanded);
    }

    #[test]
    fn toggle_expand_flips_true_to_false_and_resets_payload_scroll() {
        let mut s = CallLogState {
            expanded: true,
            payload_scroll: 5,
            ..Default::default()
        };
        s.toggle_expand();
        assert!(!s.expanded);
        assert_eq!(s.payload_scroll, 0);
    }

    #[test]
    fn default_state_is_zeroed() {
        let s = CallLogState::default();
        assert_eq!(s.selected, 0);
        assert!(!s.expanded);
        assert_eq!(s.list_scroll, 0);
        assert_eq!(s.payload_scroll, 0);
    }

    #[test]
    fn clamp_to_total_drops_selection_past_end() {
        let mut s = state_at(20);
        s.clamp_to_total(10);
        assert_eq!(s.selected, 9);
    }

    #[test]
    fn clamp_to_total_zero_total_resets_selection() {
        let mut s = state_at(5);
        s.clamp_to_total(0);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn scroll_payload_up_saturates_at_zero() {
        let mut s = CallLogState::default();
        s.scroll_payload_up();
        assert_eq!(s.payload_scroll, 0);
    }

    #[test]
    fn scroll_payload_down_increments() {
        let mut s = CallLogState::default();
        s.scroll_payload_down();
        s.scroll_payload_down();
        assert_eq!(s.payload_scroll, 2);
    }

    // --- Issue #886: live-tail follow mode ---

    #[test]
    fn follow_tail_defaults_to_off() {
        assert!(!CallLogState::default().follow_tail);
    }

    #[test]
    fn toggle_follow_tail_flips_state() {
        let mut s = CallLogState::default();
        s.toggle_follow_tail();
        assert!(s.follow_tail);
        s.toggle_follow_tail();
        assert!(!s.follow_tail);
    }

    #[test]
    fn reconcile_follow_tail_on_advances_to_last_when_count_grows() {
        let mut s = CallLogState {
            follow_tail: true,
            ..Default::default()
        };
        s.reconcile_follow_tail(10);
        assert_eq!(s.selected, 9, "follow-tail must snap to the newest entry");
    }

    #[test]
    fn reconcile_follow_tail_off_leaves_selection() {
        let mut s = state_at(2); // follow_tail false by default
        s.reconcile_follow_tail(10);
        assert_eq!(s.selected, 2, "follow-tail off must not move the cursor");
    }

    #[test]
    fn reconcile_follow_tail_on_without_growth_leaves_selection() {
        let mut s = CallLogState {
            follow_tail: true,
            selected: 3,
            last_seen_total: 10,
            ..Default::default()
        };
        s.reconcile_follow_tail(10); // no growth since previous tick
        assert_eq!(s.selected, 3);
    }

    #[test]
    fn disable_follow_tail_clears_flag() {
        let mut s = CallLogState {
            follow_tail: true,
            ..Default::default()
        };
        s.disable_follow_tail();
        assert!(!s.follow_tail);
    }
}
