//! Single-slot undo buffer with a 5-second expiry window.
//!
//! `push` stores a snapshot tagged with the wall-clock instant of the
//! delete; `take_if_fresh` returns the snapshot only when `now - deleted_at
//! < UNDO_WINDOW`. A second `push` within the window overwrites the first
//! (spec §3 Q9). Wall-clock `now` is passed in by the caller — the buffer
//! does not depend on a [`super::clock::Clock`] directly.

use std::time::{Duration, Instant};

use super::entry_state::EntryState;

pub const UNDO_WINDOW: Duration = Duration::from_secs(5);

pub struct UndoSnapshot {
    pub deleted_id: String,
    pub entry: EntryState,
    pub original_index: Option<usize>,
    pub deleted_at: Instant,
}

#[derive(Default)]
pub struct UndoBuffer {
    slot: Option<UndoSnapshot>,
}

impl UndoBuffer {
    pub fn new() -> Self {
        Self { slot: None }
    }

    pub fn push(
        &mut self,
        deleted_id: String,
        entry: EntryState,
        original_index: Option<usize>,
        now: Instant,
    ) -> Option<String> {
        let displaced = self.slot.take().map(|s| s.deleted_id);
        self.slot = Some(UndoSnapshot {
            deleted_id,
            entry,
            original_index,
            deleted_at: now,
        });
        displaced
    }

    pub fn take_if_fresh(&mut self, now: Instant) -> Option<UndoSnapshot> {
        let snap = self.slot.take()?;
        if now.duration_since(snap.deleted_at) < UNDO_WINDOW {
            Some(snap)
        } else {
            None
        }
    }

    pub fn is_active(&self, now: Instant) -> bool {
        match &self.slot {
            Some(s) => now.duration_since(s.deleted_at) < UNDO_WINDOW,
            None => false,
        }
    }

    pub fn current_label(&self) -> Option<&str> {
        self.slot.as_ref().map(|s| s.deleted_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::super::clock::{Clock, FakeClock};
    use super::super::entry_state::EntryState;
    use super::*;

    fn make_entry(id: &str) -> EntryState {
        EntryState::build("agents", id, &[], None)
    }

    #[test]
    fn empty_take_returns_none() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        assert!(buf.take_if_fresh(c.now()).is_none());
    }

    #[test]
    fn push_then_take_within_window_returns_state() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), Some(0), c.now());
        c.advance(Duration::from_secs(3));
        let snap = buf.take_if_fresh(c.now()).expect("fresh");
        assert_eq!(snap.deleted_id, "a");
        assert_eq!(snap.original_index, Some(0));
    }

    #[test]
    fn push_then_take_after_window_returns_none() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), None, c.now());
        c.advance(Duration::from_secs(5) + Duration::from_nanos(1));
        assert!(buf.take_if_fresh(c.now()).is_none());
    }

    #[test]
    fn is_active_returns_true_within_window() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), None, c.now());
        assert!(buf.is_active(c.now()));
        c.advance(Duration::from_millis(4_999));
        assert!(buf.is_active(c.now()));
    }

    #[test]
    fn is_active_returns_false_after_window() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), None, c.now());
        c.advance(Duration::from_secs(5));
        assert!(!buf.is_active(c.now()));
    }

    #[test]
    fn is_active_returns_false_when_empty() {
        let buf = UndoBuffer::new();
        let c = FakeClock::new();
        assert!(!buf.is_active(c.now()));
    }

    #[test]
    fn take_clears_slot() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), None, c.now());
        let _ = buf.take_if_fresh(c.now());
        assert!(buf.take_if_fresh(c.now()).is_none());
    }

    #[test]
    fn second_push_overwrites_first_and_returns_displaced_id() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), None, c.now());
        c.advance(Duration::from_secs(2));
        let displaced = buf.push("b".into(), make_entry("b"), None, c.now());
        assert_eq!(displaced.as_deref(), Some("a"));
        c.advance(Duration::from_secs(2));
        let snap = buf.take_if_fresh(c.now()).expect("b fresh");
        assert_eq!(snap.deleted_id, "b");
    }

    #[test]
    fn window_boundary_is_exclusive() {
        let mut buf = UndoBuffer::new();
        let c = FakeClock::new();
        buf.push("a".into(), make_entry("a"), None, c.now());
        c.advance(UNDO_WINDOW);
        assert!(buf.take_if_fresh(c.now()).is_none());
    }
}
