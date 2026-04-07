#![allow(dead_code)]
use std::fmt;

/// Unique identifier for a focusable pane within a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FocusId(pub &'static str);

impl fmt::Display for FocusId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Manages focus cycling within a screen's declared panes.
#[derive(Debug, Clone)]
pub struct FocusRing {
    panes: Vec<FocusId>,
    current: usize,
}

impl FocusRing {
    /// Create a new FocusRing. Panics if `panes` is empty.
    pub fn new(panes: Vec<FocusId>) -> Self {
        assert!(!panes.is_empty(), "FocusRing requires at least one pane");
        Self { panes, current: 0 }
    }

    /// The currently focused pane.
    pub fn current(&self) -> FocusId {
        self.panes[self.current]
    }

    /// Cycle to the next pane (wraps around). Returns the new focus.
    pub fn next(&mut self) -> FocusId {
        if !self.panes.is_empty() {
            self.current = (self.current + 1) % self.panes.len();
        }
        self.current()
    }

    /// Cycle to the previous pane (wraps around). Returns the new focus.
    pub fn previous(&mut self) -> FocusId {
        if !self.panes.is_empty() {
            self.current = if self.current == 0 {
                self.panes.len() - 1
            } else {
                self.current - 1
            };
        }
        self.current()
    }

    /// Jump to a specific pane by id. Returns true if found.
    pub fn set(&mut self, id: FocusId) -> bool {
        if let Some(pos) = self.panes.iter().position(|p| *p == id) {
            self.current = pos;
            true
        } else {
            false
        }
    }

    /// Check if a given pane is currently focused.
    pub fn is_focused(&self, id: FocusId) -> bool {
        self.current() == id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_ring_new_sets_current_to_first_pane() {
        let ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        assert_eq!(ring.current(), FocusId("a"));
    }

    #[test]
    fn focus_ring_new_with_single_pane_current_is_that_pane() {
        let ring = FocusRing::new(vec![FocusId("only")]);
        assert_eq!(ring.current(), FocusId("only"));
    }

    #[test]
    fn focus_ring_next_advances_to_second_pane() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        let next = ring.next();
        assert_eq!(next, FocusId("b"));
        assert_eq!(ring.current(), FocusId("b"));
    }

    #[test]
    fn focus_ring_next_wraps_from_last_to_first() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        ring.next(); // -> b
        ring.next(); // -> c
        let wrapped = ring.next(); // -> a
        assert_eq!(wrapped, FocusId("a"));
    }

    #[test]
    fn focus_ring_next_on_single_pane_stays_at_same_pane() {
        let mut ring = FocusRing::new(vec![FocusId("only")]);
        let next = ring.next();
        assert_eq!(next, FocusId("only"));
    }

    #[test]
    fn focus_ring_previous_wraps_from_first_to_last() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        let prev = ring.previous();
        assert_eq!(prev, FocusId("c"));
    }

    #[test]
    fn focus_ring_previous_moves_back_one_step() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        ring.next(); // -> b
        ring.next(); // -> c
        let prev = ring.previous(); // -> b
        assert_eq!(prev, FocusId("b"));
    }

    #[test]
    fn focus_ring_previous_on_single_pane_stays_at_same_pane() {
        let mut ring = FocusRing::new(vec![FocusId("only")]);
        assert_eq!(ring.previous(), FocusId("only"));
    }

    #[test]
    fn focus_ring_next_then_previous_returns_to_origin() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        ring.next();
        ring.previous();
        assert_eq!(ring.current(), FocusId("a"));
    }

    #[test]
    fn focus_ring_set_valid_id_returns_true_and_updates_current() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b"), FocusId("c")]);
        let result = ring.set(FocusId("c"));
        assert!(result);
        assert_eq!(ring.current(), FocusId("c"));
    }

    #[test]
    fn focus_ring_set_unknown_id_returns_false_and_leaves_current_unchanged() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b")]);
        let result = ring.set(FocusId("z"));
        assert!(!result);
        assert_eq!(ring.current(), FocusId("a"));
    }

    #[test]
    fn focus_ring_is_focused_returns_true_for_current_pane() {
        let ring = FocusRing::new(vec![FocusId("a"), FocusId("b")]);
        assert!(ring.is_focused(FocusId("a")));
    }

    #[test]
    fn focus_ring_is_focused_returns_false_for_non_current_pane() {
        let ring = FocusRing::new(vec![FocusId("a"), FocusId("b")]);
        assert!(!ring.is_focused(FocusId("b")));
    }

    #[test]
    fn focus_ring_is_focused_updates_correctly_after_next() {
        let mut ring = FocusRing::new(vec![FocusId("a"), FocusId("b")]);
        ring.next();
        assert!(!ring.is_focused(FocusId("a")));
        assert!(ring.is_focused(FocusId("b")));
    }

    #[test]
    fn focus_ring_full_cycle_returns_to_start() {
        let panes = vec![FocusId("a"), FocusId("b"), FocusId("c"), FocusId("d")];
        let len = panes.len();
        let mut ring = FocusRing::new(panes);
        for _ in 0..len {
            ring.next();
        }
        assert_eq!(ring.current(), FocusId("a"));
    }

    #[test]
    fn focus_ring_full_reverse_cycle_returns_to_start() {
        let panes = vec![FocusId("a"), FocusId("b"), FocusId("c")];
        let len = panes.len();
        let mut ring = FocusRing::new(panes);
        for _ in 0..len {
            ring.previous();
        }
        assert_eq!(ring.current(), FocusId("a"));
    }

    #[test]
    #[should_panic(expected = "FocusRing requires at least one pane")]
    fn focus_ring_empty_panes_panics() {
        FocusRing::new(vec![]);
    }
}
