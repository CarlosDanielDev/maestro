//! Roadmap screen state (#329).

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::tui::screens::roadmap::dep_levels::dep_levels;
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
    pub offset: usize,
    pub filters: Filters,
    pub editing_filter: Option<FilterField>,
    pub is_loading: bool,
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
            offset: 0,
            filters: Filters::default(),
            editing_filter: None,
            is_loading: false,
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
        if max == 0 {
            self.offset = 0;
        } else {
            self.offset = self.offset.min(self.rendered_row_count().saturating_sub(1));
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

    pub fn page_down(&mut self, rows: usize) {
        if self.entries.is_empty() {
            self.cursor = 0;
            return;
        }
        self.cursor = (self.cursor + rows).min(self.entries.len() - 1);
    }

    pub fn page_up(&mut self, rows: usize) {
        self.cursor = self.cursor.saturating_sub(rows);
    }

    pub fn clamp_offset(&mut self, visible_rows: usize, total_rows: usize) {
        let focused_row = self.cursor.min(total_rows.saturating_sub(1));
        self.clamp_offset_to_row(focused_row, visible_rows, total_rows);
    }

    pub fn clamp_offset_to_cursor(&mut self, visible_rows: usize) {
        self.clamp_offset_to_row(
            self.focused_rendered_row(),
            visible_rows,
            self.rendered_row_count(),
        );
    }

    pub fn clamp_offset_to_row(
        &mut self,
        focused_row: usize,
        visible_rows: usize,
        total_rows: usize,
    ) {
        if visible_rows == 0 || total_rows <= visible_rows {
            self.offset = 0;
            return;
        }

        let focused_row = focused_row.min(total_rows.saturating_sub(1));
        if focused_row < self.offset {
            self.offset = focused_row;
        } else if focused_row >= self.offset.saturating_add(visible_rows) {
            self.offset = focused_row + 1 - visible_rows;
        }

        let max_offset = total_rows.saturating_sub(visible_rows);
        self.offset = self.offset.min(max_offset);
    }

    pub fn rendered_row_count(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| 1 + self.expanded_row_count(entry))
            .sum()
    }

    pub fn focused_rendered_row(&self) -> usize {
        self.entries
            .iter()
            .take(self.cursor)
            .map(|entry| 1 + self.expanded_row_count(entry))
            .sum()
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

    fn expanded_row_count(&self, entry: &RoadmapEntry) -> usize {
        if !self.is_expanded(entry.milestone.number) {
            return 0;
        }

        let visible: Vec<_> = entry
            .issues
            .iter()
            .filter(|issue| self.filters.matches(issue))
            .collect();
        if visible.is_empty() {
            return 1;
        }

        let visible_set: HashSet<u64> = visible.iter().map(|issue| issue.number).collect();
        let inputs: Vec<_> = visible
            .iter()
            .map(|issue| {
                (
                    issue.number,
                    issue
                        .all_blockers()
                        .into_iter()
                        .filter(|blocker| visible_set.contains(blocker))
                        .collect(),
                )
            })
            .collect();
        let level_count = dep_levels(&inputs).map(|levels| levels.len()).unwrap_or(1);
        level_count + visible.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::types::Milestone;

    fn entry(num: u64, semver: SemVer) -> RoadmapEntry {
        RoadmapEntry {
            milestone: Milestone {
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
    fn clamp_offset_shifts_down_when_cursor_below_visible_window() {
        let mut s = RoadmapScreen::new();
        s.cursor = 12;
        s.offset = 0;

        s.clamp_offset(10, 30);

        assert_eq!(s.offset, 3);
    }

    #[test]
    fn clamp_offset_shifts_up_when_cursor_above_visible_window() {
        let mut s = RoadmapScreen::new();
        s.cursor = 4;
        s.offset = 10;

        s.clamp_offset(10, 30);

        assert_eq!(s.offset, 4);
    }

    #[test]
    fn clamp_offset_stays_zero_when_total_rows_fit() {
        let mut s = RoadmapScreen::new();
        s.cursor = 4;
        s.offset = 3;

        s.clamp_offset(10, 8);

        assert_eq!(s.offset, 0);
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
