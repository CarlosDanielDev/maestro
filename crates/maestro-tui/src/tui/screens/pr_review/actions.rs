//! Accept / reject input for the review concerns panel (#327).
//!
//! Returns a `ConcernAction` for the App to fold into state. The App then
//! drives `ChangeApplier` for accepted concerns.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::review::types::{Concern, ConcernStatus};
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcernAction {
    None,
    Accept,
    Reject,
    CursorDown,
    CursorUp,
    Dismiss,
}

pub fn handle_key(key: KeyEvent) -> ConcernAction {
    match key.code {
        KeyCode::Char('a') => ConcernAction::Accept,
        KeyCode::Char('r') => ConcernAction::Reject,
        KeyCode::Down | KeyCode::Char('j') => ConcernAction::CursorDown,
        KeyCode::Up | KeyCode::Char('k') => ConcernAction::CursorUp,
        KeyCode::Esc | KeyCode::Char('q') => ConcernAction::Dismiss,
        _ => ConcernAction::None,
    }
}

/// Apply an action to a list of concerns at a given cursor. Returns the
/// updated cursor and whether any state mutation occurred (so the caller
/// can decide whether to drive `ChangeApplier`).
pub fn apply(concerns: &mut [Concern], cursor: usize, action: ConcernAction) -> (usize, bool) {
    match action {
        ConcernAction::None | ConcernAction::Dismiss => (cursor, false),
        ConcernAction::CursorDown => {
            let next = if concerns.is_empty() {
                0
            } else {
                cursor
                    .min(concerns.len() - 1)
                    .saturating_add(1)
                    .min(concerns.len() - 1)
            };
            (next, false)
        }
        ConcernAction::CursorUp => (cursor.saturating_sub(1), false),
        ConcernAction::Accept => {
            let Some(c) = concerns.get_mut(cursor) else {
                return (cursor, false);
            };
            if c.transition(ConcernStatus::Accepted).is_ok() {
                (cursor, true)
            } else {
                (cursor, false)
            }
        }
        ConcernAction::Reject => {
            let Some(c) = concerns.get_mut(cursor) else {
                return (cursor, false);
            };
            if c.transition(ConcernStatus::Rejected).is_ok() {
                (cursor, true)
            } else {
                (cursor, false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::types::{ConcernId, Severity};
    use crossterm::event::KeyModifiers;
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn pending_concern() -> Concern {
        Concern {
            id: ConcernId::new(),
            severity: Severity::Warning,
            file: PathBuf::from("a.rs"),
            line: Some(1),
            message: "x".into(),
            suggested_diff: None,
            status: ConcernStatus::Pending,
        }
    }

    #[test]
    fn key_a_returns_accept() {
        assert_eq!(handle_key(key(KeyCode::Char('a'))), ConcernAction::Accept);
    }

    #[test]
    fn key_r_returns_reject() {
        assert_eq!(handle_key(key(KeyCode::Char('r'))), ConcernAction::Reject);
    }

    #[test]
    fn key_j_returns_cursor_down() {
        assert_eq!(
            handle_key(key(KeyCode::Char('j'))),
            ConcernAction::CursorDown
        );
    }

    #[test]
    fn key_esc_returns_dismiss() {
        assert_eq!(handle_key(key(KeyCode::Esc)), ConcernAction::Dismiss);
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(handle_key(key(KeyCode::F(5))), ConcernAction::None);
    }

    #[test]
    fn apply_accept_transitions_pending_to_accepted() {
        let mut concerns = vec![pending_concern()];
        let (cursor, mutated) = apply(&mut concerns, 0, ConcernAction::Accept);
        assert_eq!(cursor, 0);
        assert!(mutated);
        assert_eq!(concerns[0].status, ConcernStatus::Accepted);
    }

    #[test]
    fn apply_reject_transitions_pending_to_rejected() {
        let mut concerns = vec![pending_concern()];
        let (_, mutated) = apply(&mut concerns, 0, ConcernAction::Reject);
        assert!(mutated);
        assert_eq!(concerns[0].status, ConcernStatus::Rejected);
    }

    #[test]
    fn apply_accept_on_already_accepted_is_no_op() {
        let mut concerns = vec![pending_concern()];
        apply(&mut concerns, 0, ConcernAction::Accept);
        let (_, mutated) = apply(&mut concerns, 0, ConcernAction::Accept);
        assert!(!mutated);
    }

    #[test]
    fn apply_cursor_down_on_empty_returns_zero() {
        let mut concerns: Vec<Concern> = Vec::new();
        let (cursor, _) = apply(&mut concerns, 0, ConcernAction::CursorDown);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn apply_cursor_down_stops_at_last() {
        let mut concerns = vec![pending_concern(), pending_concern()];
        let (c1, _) = apply(&mut concerns, 0, ConcernAction::CursorDown);
        let (c2, _) = apply(&mut concerns, c1, ConcernAction::CursorDown);
        let (c3, _) = apply(&mut concerns, c2, ConcernAction::CursorDown);
        assert_eq!(c3, 1);
    }
}
