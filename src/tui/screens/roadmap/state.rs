//! Roadmap screen state (#329).

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::tui::screens::roadmap::types::{Filters, RoadmapEntry, SemVer};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterField {
    Label,
    Assignee,
    Status,
}

#[derive(Debug, Clone)]
pub struct RoadmapScreen {
    pub entries: Vec<RoadmapEntry>,
    pub expanded: HashSet<u64>,
    pub cursor: usize,
    pub filters: Filters,
    pub editing_filter: Option<FilterField>,
}

impl Default for RoadmapScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl RoadmapScreen {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            expanded: HashSet::new(),
            cursor: 0,
            filters: Filters::default(),
            editing_filter: None,
        }
    }

    /// Replace entries; sorts by descending semver (newest first), preserves
    /// expansion state by milestone number.
    pub fn set_entries(&mut self, mut entries: Vec<RoadmapEntry>) {
        entries.sort_by(|a, b| {
            b.semver
                .cmp(&a.semver)
                .then_with(|| a.milestone.number.cmp(&b.milestone.number))
        });
        let max = entries.len();
        self.entries = entries;
        if self.cursor >= max {
            self.cursor = max.saturating_sub(1);
        }
    }

    pub fn toggle_expand(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.cursor) else {
            return false;
        };
        let n = entry.milestone.number;
        if !self.expanded.insert(n) {
            self.expanded.remove(&n);
        }
        true
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }

    pub fn cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn focused_milestone(&self) -> Option<&RoadmapEntry> {
        self.entries.get(self.cursor)
    }

    pub fn focused_milestone_semver(&self) -> Option<SemVer> {
        self.focused_milestone().map(|e| e.semver)
    }

    pub fn is_expanded(&self, milestone_number: u64) -> bool {
        self.expanded.contains(&milestone_number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::types::GhMilestone;

    fn entry(num: u64, semver: SemVer) -> RoadmapEntry {
        RoadmapEntry {
            milestone: GhMilestone {
                number: num,
                title: format!("v{}.{}.{}", semver.major, semver.minor, semver.patch),
                description: String::new(),
                state: "open".into(),
                open_issues: 0,
                closed_issues: 0,
            },
            semver,
            issues: Vec::new(),
        }
    }

    #[test]
    fn set_entries_sorts_descending_semver() {
        let mut s = RoadmapScreen::new();
        s.set_entries(vec![
            entry(
                1,
                SemVer {
                    major: 0,
                    minor: 15,
                    patch: 2,
                },
            ),
            entry(
                2,
                SemVer {
                    major: 0,
                    minor: 16,
                    patch: 0,
                },
            ),
            entry(
                3,
                SemVer {
                    major: 1,
                    minor: 0,
                    patch: 0,
                },
            ),
        ]);
        assert_eq!(s.entries[0].milestone.number, 3); // v1.0.0
        assert_eq!(s.entries[1].milestone.number, 2); // v0.16.0
        assert_eq!(s.entries[2].milestone.number, 1); // v0.15.2
    }

    #[test]
    fn toggle_expand_flips_membership() {
        let mut s = RoadmapScreen::new();
        s.set_entries(vec![entry(
            1,
            SemVer {
                major: 0,
                minor: 1,
                patch: 0,
            },
        )]);
        assert!(s.toggle_expand());
        assert!(s.is_expanded(1));
        assert!(s.toggle_expand());
        assert!(!s.is_expanded(1));
    }

    #[test]
    fn cursor_down_stops_at_last_entry() {
        let mut s = RoadmapScreen::new();
        s.set_entries(vec![
            entry(
                1,
                SemVer {
                    major: 0,
                    minor: 1,
                    patch: 0,
                },
            ),
            entry(
                2,
                SemVer {
                    major: 0,
                    minor: 2,
                    patch: 0,
                },
            ),
        ]);
        s.cursor_down();
        s.cursor_down();
        s.cursor_down();
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn cursor_up_does_not_underflow() {
        let mut s = RoadmapScreen::new();
        s.cursor_up();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn focused_milestone_returns_correct_entry() {
        let mut s = RoadmapScreen::new();
        s.set_entries(vec![
            entry(
                1,
                SemVer {
                    major: 0,
                    minor: 2,
                    patch: 0,
                },
            ),
            entry(
                2,
                SemVer {
                    major: 0,
                    minor: 1,
                    patch: 0,
                },
            ),
        ]);
        s.cursor_down();
        assert_eq!(s.focused_milestone().expect("focused").milestone.number, 2);
    }
}
