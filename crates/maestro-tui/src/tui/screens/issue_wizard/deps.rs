//! #1282 Blocked-by picker: the `/` filter over number, title and labels.
//! The filter narrows what the list shows; it never changes `dep_checked`,
//! so the selected chips always say what will be written as `## Blocked By`.

use super::{IssueWizardScreen, ScreenAction};
use crossterm::event::{Event, KeyCode};

impl IssueWizardScreen {
    /// Indexes into `dep_issues` that match the filter (all when empty).
    /// Case-insensitive substring on `#number`, the title and each label.
    pub fn dep_visible(&self) -> Vec<usize> {
        let Some(issues) = self.dep_issues.as_ref() else {
            return Vec::new();
        };
        let needle = self.dep_filter_text().trim().to_lowercase();
        issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| {
                needle.is_empty()
                    || format!("#{}", issue.number).contains(&needle)
                    || issue.title.to_lowercase().contains(&needle)
                    || issue
                        .labels
                        .iter()
                        .any(|l| l.to_lowercase().contains(&needle))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn dep_filter_text(&self) -> String {
        self.dep_filter.text()
    }

    /// `/`: type into the filter (INSERT mode).
    pub fn open_dep_filter(&mut self) {
        self.dep_filter_editing = true;
    }

    pub fn dep_filter_editing(&self) -> bool {
        self.dep_filter_editing
    }

    /// The selected issue numbers, in order (the chips).
    pub fn dep_checked_numbers(&self) -> Vec<u64> {
        self.dep_checked.iter().copied().collect()
    }

    fn dep_clamp_selection(&mut self) {
        let len = self.dep_visible().len();
        self.dep_selected = self.dep_selected.min(len.saturating_sub(1));
    }

    fn dep_move(&mut self, down: bool) {
        if down {
            if self.dep_selected + 1 < self.dep_visible().len() {
                self.dep_selected += 1;
            }
        } else {
            self.dep_selected = self.dep_selected.saturating_sub(1);
        }
    }

    pub(super) fn handle_dependencies(&mut self, event: &Event, code: KeyCode) -> ScreenAction {
        if self.dep_filter_editing {
            match code {
                // Esc clears the text, never the selection.
                KeyCode::Esc => {
                    self.dep_filter.set_text("");
                    self.dep_filter_editing = false;
                }
                KeyCode::Enter => self.dep_filter_editing = false,
                KeyCode::Down => self.dep_move(true),
                KeyCode::Up => self.dep_move(false),
                // Focus keys are not text.
                KeyCode::Tab | KeyCode::BackTab => {}
                _ => {
                    crate::tui::widgets::input::feed(self.dep_filter.area_mut(), event);
                }
            }
            self.dep_clamp_selection();
            return ScreenAction::None;
        }
        match code {
            KeyCode::Esc => {
                self.retreat();
            }
            KeyCode::Char('/') => self.open_dep_filter(),
            KeyCode::Char(' ') => self.dep_toggle_selected(),
            KeyCode::Char('j') | KeyCode::Down => self.dep_move(true),
            KeyCode::Char('k') | KeyCode::Up => self.dep_move(false),
            KeyCode::Enter => {
                self.try_advance();
            }
            _ => {}
        }
        ScreenAction::None
    }
}
