//! PRD screen state machine (#321).

#![deny(clippy::unwrap_used)]

use crate::prd::model::{Goal, Prd};
use std::time::Instant;

/// Async-feedback status shown prominently in the PRD header so the user
/// always knows what's happening without watching the activity log.
#[derive(Debug, Clone, Default)]
pub enum PrdSyncStatus {
    #[default]
    Idle,
    Syncing {
        started_at: Instant,
    },
    SyncedAt(Instant),
    Failed {
        at: Instant,
        message: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct PrdSaveStatus {
    pub last_saved: Option<Instant>,
    pub last_error: Option<String>,
}

/// Which top-level section the cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrdSection {
    Vision,
    Goals,
    NonGoals,
    CurrentState,
    Stakeholders,
    Timeline,
}

impl PrdSection {
    pub const ALL: [Self; 6] = [
        Self::Vision,
        Self::Goals,
        Self::NonGoals,
        Self::CurrentState,
        Self::Stakeholders,
        Self::Timeline,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Vision => "Vision & Purpose",
            Self::Goals => "Goals",
            Self::NonGoals => "Non-Goals",
            Self::CurrentState => "Current State",
            Self::Stakeholders => "Stakeholders",
            Self::Timeline => "Timeline",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Currently-edited target. `None` means view-mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditTarget {
    NewGoal { buffer: String },
    NewNonGoal { buffer: String },
}

/// Action returned by `input::handle_key` for the App to enact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrdAction {
    None,
    /// Persist current PRD to disk.
    Save,
    /// Sync `current_state` from GitHub.
    Sync,
    /// Export markdown to stdout via the activity log.
    Export,
    /// Pop back to previous screen.
    Back,
}

pub struct PrdScreen {
    pub focus: PrdSection,
    pub edit: Option<EditTarget>,
    pub goal_cursor: usize,
    pub non_goal_cursor: usize,
    pub dirty: bool,
    /// Sync-from-GitHub progress chip rendered in the header.
    pub sync_status: PrdSyncStatus,
    /// "Saved Xs ago" / save-failed indicator.
    pub save_status: PrdSaveStatus,
    /// Whether the screen is being viewed for the first time this session
    /// (used to render a one-line "what is this" intro).
    pub first_view: bool,
}

impl Default for PrdScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl PrdScreen {
    pub fn new() -> Self {
        Self {
            focus: PrdSection::Vision,
            edit: None,
            goal_cursor: 0,
            non_goal_cursor: 0,
            dirty: false,
            sync_status: PrdSyncStatus::Idle,
            save_status: PrdSaveStatus::default(),
            first_view: true,
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
        self.clamp_cursors_for_focus();
    }

    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
        self.clamp_cursors_for_focus();
    }

    fn clamp_cursors_for_focus(&mut self) {
        // Cursor reset when switching sections — keeps focus visible.
        if matches!(self.focus, PrdSection::Goals) {
            self.goal_cursor = 0;
        }
        if matches!(self.focus, PrdSection::NonGoals) {
            self.non_goal_cursor = 0;
        }
    }

    /// Add a new goal from the edit buffer; returns true on success.
    pub fn commit_new_goal(&mut self, prd: &mut Prd) -> bool {
        let Some(EditTarget::NewGoal { buffer }) = self.edit.take() else {
            return false;
        };
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            return false;
        }
        prd.goals.push(Goal::new(trimmed));
        self.dirty = true;
        true
    }

    /// Add a new non-goal from the edit buffer; returns true on success.
    pub fn commit_new_non_goal(&mut self, prd: &mut Prd) -> bool {
        let Some(EditTarget::NewNonGoal { buffer }) = self.edit.take() else {
            return false;
        };
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            return false;
        }
        prd.non_goals.push(trimmed.to_string());
        self.dirty = true;
        true
    }

    pub fn toggle_goal_done(&mut self, prd: &mut Prd) -> bool {
        let Some(g) = prd.goals.get_mut(self.goal_cursor) else {
            return false;
        };
        g.done = !g.done;
        self.dirty = true;
        true
    }

    pub fn delete_focused_goal(&mut self, prd: &mut Prd) -> bool {
        if self.goal_cursor >= prd.goals.len() {
            return false;
        }
        prd.goals.remove(self.goal_cursor);
        if self.goal_cursor > 0 && self.goal_cursor >= prd.goals.len() {
            self.goal_cursor -= 1;
        }
        self.dirty = true;
        true
    }

    pub fn delete_focused_non_goal(&mut self, prd: &mut Prd) -> bool {
        if self.non_goal_cursor >= prd.non_goals.len() {
            return false;
        }
        prd.non_goals.remove(self.non_goal_cursor);
        if self.non_goal_cursor > 0 && self.non_goal_cursor >= prd.non_goals.len() {
            self.non_goal_cursor -= 1;
        }
        self.dirty = true;
        true
    }

    pub fn cursor_down_in_focus(&mut self, prd: &Prd) {
        match self.focus {
            PrdSection::Goals => {
                if !prd.goals.is_empty() && self.goal_cursor < prd.goals.len() - 1 {
                    self.goal_cursor += 1;
                }
            }
            PrdSection::NonGoals => {
                if !prd.non_goals.is_empty() && self.non_goal_cursor < prd.non_goals.len() - 1 {
                    self.non_goal_cursor += 1;
                }
            }
            _ => {}
        }
    }

    pub fn cursor_up_in_focus(&mut self) {
        match self.focus {
            PrdSection::Goals => self.goal_cursor = self.goal_cursor.saturating_sub(1),
            PrdSection::NonGoals => self.non_goal_cursor = self.non_goal_cursor.saturating_sub(1),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (PrdScreen, Prd) {
        (PrdScreen::new(), Prd::new())
    }

    #[test]
    fn focus_next_cycles_through_all_sections_and_wraps() {
        let mut s = PrdScreen::new();
        for expected in PrdSection::ALL
            .iter()
            .skip(1)
            .chain(std::iter::once(&PrdSection::Vision))
        {
            s.focus_next();
            assert_eq!(s.focus, *expected);
        }
    }

    #[test]
    fn commit_new_goal_appends_and_marks_dirty() {
        let (mut s, mut p) = fresh();
        s.edit = Some(EditTarget::NewGoal {
            buffer: "Ship v1".into(),
        });
        assert!(s.commit_new_goal(&mut p));
        assert_eq!(p.goals.len(), 1);
        assert_eq!(p.goals[0].text, "Ship v1");
        assert!(s.dirty);
        assert!(s.edit.is_none());
    }

    #[test]
    fn commit_new_goal_with_empty_buffer_no_ops() {
        let (mut s, mut p) = fresh();
        s.edit = Some(EditTarget::NewGoal {
            buffer: "   ".into(),
        });
        assert!(!s.commit_new_goal(&mut p));
        assert!(p.goals.is_empty());
        assert!(!s.dirty);
    }

    #[test]
    fn commit_new_non_goal_appends_and_marks_dirty() {
        let (mut s, mut p) = fresh();
        s.edit = Some(EditTarget::NewNonGoal {
            buffer: "Multi-tenant".into(),
        });
        assert!(s.commit_new_non_goal(&mut p));
        assert_eq!(p.non_goals, vec!["Multi-tenant"]);
        assert!(s.dirty);
    }

    #[test]
    fn toggle_goal_done_flips_state() {
        let (mut s, mut p) = fresh();
        p.add_goal("First");
        assert!(!p.goals[0].done);
        assert!(s.toggle_goal_done(&mut p));
        assert!(p.goals[0].done);
        assert!(s.toggle_goal_done(&mut p));
        assert!(!p.goals[0].done);
        assert!(s.dirty);
    }

    #[test]
    fn delete_focused_goal_removes_and_clamps_cursor() {
        let (mut s, mut p) = fresh();
        p.add_goal("a");
        p.add_goal("b");
        p.add_goal("c");
        s.goal_cursor = 2;
        assert!(s.delete_focused_goal(&mut p));
        assert_eq!(p.goals.len(), 2);
        assert_eq!(s.goal_cursor, 1);
    }

    #[test]
    fn cursor_down_in_goals_stops_at_last() {
        let (mut s, mut p) = fresh();
        p.add_goal("a");
        p.add_goal("b");
        s.focus = PrdSection::Goals;
        s.cursor_down_in_focus(&p);
        s.cursor_down_in_focus(&p);
        s.cursor_down_in_focus(&p);
        assert_eq!(s.goal_cursor, 1);
    }
}
