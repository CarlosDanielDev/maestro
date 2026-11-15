//! Add/Remove modal lifecycle + tab navigation for
//! [`super::DynamicMapWidget`].
//!
//! Split out of `dynamic_map.rs` (#908). Owns `[`/`]` tab walks, modal
//! open/dispatch, entry insert/remove, and the 5-second undo replay.

use crate::tui::widgets::WidgetAction;

use super::super::super::modals::ModalAction;
use super::super::super::modals::add_entry::AddEntryModal;
use super::super::super::modals::remove_confirm::RemoveConfirmModal;
use super::super::entry_state::EntryState;
use super::{DynamicMapWidget, MapFocus, display_name_for};

impl DynamicMapWidget {
    pub(super) fn next_tab(&mut self) {
        if let Some(idx) = self.active_idx {
            let n = self.entries.len();
            if n > 0 {
                self.active_idx = Some((idx + 1) % n);
            }
        }
    }

    pub(super) fn prev_tab(&mut self) {
        if let Some(idx) = self.active_idx {
            let n = self.entries.len();
            if n > 0 {
                self.active_idx = Some(if idx == 0 { n - 1 } else { idx - 1 });
            }
        }
    }

    pub(super) fn open_add_modal(&mut self) {
        let existing: Vec<String> = self.entries.iter().map(|e| e.id.clone()).collect();
        let display_name = display_name_for(&self.section_path);
        // Curated short names (e.g. "provider", "role") read better as
        // "Add role" than "Add role entry"; the raw section_path
        // fallback still gets the trailing " entry" so opaque paths
        // remain self-describing.
        let title = if display_name == self.section_path.as_str() {
            format!("Add {display_name} entry")
        } else {
            format!("Add {display_name}")
        };
        self.add_modal = Some(AddEntryModal::new(title, existing));
        self.focus = MapFocus::AddModal;
    }

    pub(super) fn open_remove_modal(&mut self) {
        let label = self
            .active_entry()
            .map(|e| format!("{}.{}", self.section_path, e.id))
            .unwrap_or_default();
        self.remove_modal = Some(RemoveConfirmModal::new(label));
        self.focus = MapFocus::RemoveConfirm;
    }

    pub(super) fn dispatch_add(&mut self, action: ModalAction) -> WidgetAction {
        match action {
            ModalAction::None => WidgetAction::None,
            ModalAction::Cancel => {
                self.add_modal = None;
                self.focus = MapFocus::SubtabStrip;
                WidgetAction::None
            }
            ModalAction::Submit { id } => {
                self.add_modal = None;
                self.insert_entry(id);
                WidgetAction::Changed
            }
        }
    }

    pub(super) fn dispatch_remove(&mut self, action: ModalAction) -> WidgetAction {
        match action {
            ModalAction::None => WidgetAction::None,
            ModalAction::Cancel => {
                self.remove_modal = None;
                self.focus = MapFocus::SubtabStrip;
                WidgetAction::None
            }
            ModalAction::Submit { .. } => {
                self.remove_modal = None;
                self.remove_active();
                WidgetAction::Changed
            }
        }
    }

    fn insert_entry(&mut self, id: String) {
        let new_entry = EntryState::build(&self.section_path, id, self.entry_fields, None);
        let new_id = new_entry.id.clone();
        self.entries.push(new_entry);
        self.entries.sort_by(|a, b| a.id.cmp(&b.id));
        self.active_idx = self.entries.iter().position(|e| e.id == new_id);
        self.focus = MapFocus::EntryField(0);
    }

    fn remove_active(&mut self) {
        let Some(idx) = self.active_idx else {
            return;
        };
        let entry = self.entries.remove(idx);
        let id = entry.id.clone();
        self.undo.push(id, entry, Some(idx), self.clock.now());
        self.active_idx = if self.entries.is_empty() {
            None
        } else {
            Some(idx.min(self.entries.len() - 1))
        };
        self.focus = MapFocus::SubtabStrip;
    }

    pub(super) fn attempt_undo(&mut self) {
        let now = self.clock.now();
        if let Some(snap) = self.undo.take_if_fresh(now) {
            let target = snap.original_index.unwrap_or(self.entries.len());
            let id = snap.entry.id.clone();
            self.entries
                .insert(target.min(self.entries.len()), snap.entry);
            self.entries.sort_by(|a, b| a.id.cmp(&b.id));
            self.active_idx = self.entries.iter().position(|e| e.id == id);
        }
    }
}
