//! `DynamicRowsWidget` — row-table primitive for `FieldKind::VecOfStruct`.
//! Supports Add/Remove with the same modal pair as `DynamicMapWidget` plus
//! `Alt+↑/↓` reorder.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};

use crate::config::schema::{FieldKind, FieldSchema};
use crate::tui::theme::Theme;
use crate::tui::widgets::{WidgetAction, WidgetKind};

use super::super::modals::ModalAction;
use super::super::modals::add_entry::AddEntryModal;
use super::super::modals::remove_confirm::RemoveConfirmModal;
use super::clock::{Clock, SystemClock};
use super::entry_state::EntryState;
use super::undo::UndoBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowFocus {
    Row(usize),
    AddModal,
    RemoveConfirm,
    Empty,
}

pub struct DynamicRowsWidget {
    pub label: String,
    pub section_path: String,
    pub entry_fields: &'static [FieldSchema],
    rows: Vec<EntryState>,
    focus: RowFocus,
    add_modal: Option<AddEntryModal>,
    remove_modal: Option<RemoveConfirmModal>,
    pending_remove_idx: Option<usize>,
    undo: UndoBuffer,
    clock: Arc<dyn Clock>,
}

impl DynamicRowsWidget {
    pub fn new(
        label: impl Into<String>,
        section_path: impl Into<String>,
        entry_fields: &'static [FieldSchema],
        existing: Option<&toml::Value>,
    ) -> Self {
        Self::with_clock(
            label,
            section_path,
            entry_fields,
            existing,
            Arc::new(SystemClock),
        )
    }

    pub fn with_clock(
        label: impl Into<String>,
        section_path: impl Into<String>,
        entry_fields: &'static [FieldSchema],
        existing: Option<&toml::Value>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let label = label.into();
        let section_path = section_path.into();
        let mut rows = Vec::new();
        if let Some(arr) = existing.and_then(|v| v.as_array()) {
            for (idx, item) in arr.iter().enumerate() {
                rows.push(EntryState::build(
                    &section_path,
                    idx.to_string(),
                    entry_fields,
                    Some(item),
                ));
            }
        }
        let focus = if rows.is_empty() {
            RowFocus::Empty
        } else {
            RowFocus::Row(0)
        };
        Self {
            label,
            section_path,
            entry_fields,
            rows,
            focus,
            add_modal: None,
            remove_modal: None,
            pending_remove_idx: None,
            undo: UndoBuffer::new(),
            clock,
        }
    }

    pub fn rows(&self) -> &[EntryState] {
        &self.rows
    }

    pub fn focused_row(&self) -> Option<usize> {
        match self.focus {
            RowFocus::Row(n) => Some(n),
            _ => None,
        }
    }

    pub fn focus(&self) -> &RowFocus {
        &self.focus
    }

    pub fn undo_active(&self) -> bool {
        self.undo.is_active(self.clock.now())
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        if let Some(modal) = self.add_modal.as_mut() {
            let action = modal.handle_input(key);
            return self.dispatch_add(action);
        }
        if let Some(modal) = self.remove_modal.as_mut() {
            let action = modal.handle_input(key);
            return self.dispatch_remove(action);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('a'), _) => {
                self.open_add_modal();
                WidgetAction::None
            }
            (KeyCode::Char('d'), _) if !self.rows.is_empty() => {
                self.open_remove_modal();
                WidgetAction::None
            }
            (KeyCode::Char('u'), _) => {
                self.attempt_undo();
                WidgetAction::None
            }
            (KeyCode::Up, m) if m.contains(KeyModifiers::ALT) => {
                self.swap_up();
                WidgetAction::Changed
            }
            (KeyCode::Down, m) if m.contains(KeyModifiers::ALT) => {
                self.swap_down();
                WidgetAction::Changed
            }
            (KeyCode::Up, _) => {
                self.move_focus(-1);
                WidgetAction::None
            }
            (KeyCode::Down, _) => {
                self.move_focus(1);
                WidgetAction::None
            }
            _ => WidgetAction::None,
        }
    }

    /// Walk row focus one step up without consuming the outer
    /// SettingsScreen Up arrow. Returns true when focus moved so the
    /// outer can leave `field_index` alone.
    pub fn try_focus_prev(&mut self) -> bool {
        if let RowFocus::Row(n) = self.focus
            && n > 0
        {
            self.focus = RowFocus::Row(n - 1);
            return true;
        }
        false
    }

    /// Mirror of [`try_focus_prev`] for the Down arrow.
    pub fn try_focus_next(&mut self) -> bool {
        if let RowFocus::Row(n) = self.focus
            && n + 1 < self.rows.len()
        {
            self.focus = RowFocus::Row(n + 1);
            return true;
        }
        false
    }

    fn move_focus(&mut self, delta: i32) {
        if let RowFocus::Row(n) = self.focus {
            let len = self.rows.len() as i32;
            if len == 0 {
                return;
            }
            let new = (n as i32 + delta).clamp(0, len - 1);
            self.focus = RowFocus::Row(new as usize);
        }
    }

    fn swap_up(&mut self) -> bool {
        if let RowFocus::Row(n) = self.focus
            && n > 0
        {
            self.rows.swap(n, n - 1);
            self.focus = RowFocus::Row(n - 1);
            return true;
        }
        false
    }

    fn swap_down(&mut self) -> bool {
        if let RowFocus::Row(n) = self.focus
            && n + 1 < self.rows.len()
        {
            self.rows.swap(n, n + 1);
            self.focus = RowFocus::Row(n + 1);
            return true;
        }
        false
    }

    fn open_add_modal(&mut self) {
        let existing: Vec<String> = self.rows.iter().map(|e| e.id.clone()).collect();
        self.add_modal = Some(AddEntryModal::new(
            format!("Add {} row", self.section_path),
            existing,
        ));
        self.focus = RowFocus::AddModal;
    }

    fn open_remove_modal(&mut self) {
        let n = self.focused_row().unwrap_or(0);
        let label = format!("{}[{}]", self.section_path, n);
        self.pending_remove_idx = Some(n);
        self.remove_modal = Some(RemoveConfirmModal::new(label));
        self.focus = RowFocus::RemoveConfirm;
    }

    fn dispatch_add(&mut self, action: ModalAction) -> WidgetAction {
        match action {
            ModalAction::None => WidgetAction::None,
            ModalAction::Cancel => {
                self.add_modal = None;
                self.focus = if self.rows.is_empty() {
                    RowFocus::Empty
                } else {
                    RowFocus::Row(0)
                };
                WidgetAction::None
            }
            ModalAction::Submit { id } => {
                self.add_modal = None;
                self.insert_row(id);
                WidgetAction::Changed
            }
        }
    }

    fn dispatch_remove(&mut self, action: ModalAction) -> WidgetAction {
        match action {
            ModalAction::None => WidgetAction::None,
            ModalAction::Cancel => {
                self.remove_modal = None;
                let restore = self
                    .pending_remove_idx
                    .take()
                    .filter(|n| *n < self.rows.len());
                self.focus = restore
                    .map(RowFocus::Row)
                    .unwrap_or(if self.rows.is_empty() {
                        RowFocus::Empty
                    } else {
                        RowFocus::Row(0)
                    });
                WidgetAction::None
            }
            ModalAction::Submit { .. } => {
                self.remove_modal = None;
                self.remove_focused();
                WidgetAction::Changed
            }
        }
    }

    fn insert_row(&mut self, id: String) {
        let mut entry = EntryState::build(&self.section_path, id.clone(), self.entry_fields, None);
        // VecOfStruct rows have no identity key, so the typed identifier
        // would otherwise vanish on submit. Mirror it into the first String
        // column so the user sees a labeled row instead of an empty cell.
        if let Some(idx) = self
            .entry_fields
            .iter()
            .position(|fs| matches!(fs.kind, FieldKind::String))
            && let Some(sf) = entry.fields.get_mut(idx)
            && let WidgetKind::TextInput(t) = &mut sf.widget
        {
            t.value = id;
        }
        let new_idx = self.rows.len();
        self.rows.push(entry);
        self.focus = RowFocus::Row(new_idx);
    }

    fn remove_focused(&mut self) {
        let Some(idx) = self.pending_remove_idx.take() else {
            return;
        };
        if idx >= self.rows.len() {
            return;
        }
        let entry = self.rows.remove(idx);
        let id = entry.id.clone();
        self.undo.push(id, entry, Some(idx), self.clock.now());
        self.focus = if self.rows.is_empty() {
            RowFocus::Empty
        } else {
            RowFocus::Row(idx.min(self.rows.len() - 1))
        };
    }

    fn attempt_undo(&mut self) {
        let now = self.clock.now();
        if let Some(snap) = self.undo.take_if_fresh(now) {
            let idx = snap.original_index.unwrap_or(self.rows.len());
            let idx = idx.min(self.rows.len());
            self.rows.insert(idx, snap.entry);
            self.focus = RowFocus::Row(idx);
        }
    }

    pub fn needs_insert_mode(&self) -> bool {
        self.add_modal.is_some()
    }

    pub fn edit_hint(&self) -> &'static [(&'static str, &'static str)] {
        &[("a/d", "Add/Del"), ("Alt+↑↓", "Reorder")]
    }

    pub fn serialize_to_toml(&self) -> toml::Value {
        let arr: Vec<toml::Value> = self
            .rows
            .iter()
            .map(|e| e.to_toml(self.entry_fields))
            .collect();
        toml::Value::Array(arr)
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        super::dynamic_rows_draw::draw(self, f, area, theme, focused);
    }

    pub(super) fn undo_label(&self) -> Option<&str> {
        self.undo.current_label()
    }

    pub(super) fn add_modal(&self) -> Option<&AddEntryModal> {
        self.add_modal.as_ref()
    }

    pub(super) fn remove_modal(&self) -> Option<&RemoveConfirmModal> {
        self.remove_modal.as_ref()
    }
}
