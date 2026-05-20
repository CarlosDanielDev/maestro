//! `DynamicMapWidget` — sub-tab strip + active entry's field group for
//! `FieldKind::Map`. Owns its entries, the optional Add/Remove modals, and
//! a single-slot undo buffer with a 5-second window.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};

use crate::config::schema::FieldSchema;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetAction;

use super::super::modals::ModalAction;
use super::super::modals::add_entry::AddEntryModal;
use super::super::modals::remove_confirm::RemoveConfirmModal;
use super::clock::{Clock, SystemClock};
use super::entry_state::EntryState;
use super::identifier::validate_identifier;
use super::undo::UndoBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapFocus {
    SubtabStrip,
    EntryField(usize),
    AddModal,
    RemoveConfirm,
}

pub struct DynamicMapWidget {
    pub label: String,
    pub section_path: String,
    pub entry_fields: &'static [FieldSchema],
    entries: Vec<EntryState>,
    active_idx: Option<usize>,
    focus: MapFocus,
    add_modal: Option<AddEntryModal>,
    remove_modal: Option<RemoveConfirmModal>,
    undo: UndoBuffer,
    clock: Arc<dyn Clock>,
}

impl DynamicMapWidget {
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
        let mut entries = Vec::new();
        if let Some(table) = existing.and_then(|v| v.as_table()) {
            let mut keys: Vec<&String> = table.keys().collect();
            keys.sort();
            for k in keys {
                let entry_value = table.get(k);
                // Flattened maps share the table with scalar siblings
                // (e.g. `agents.default`). Skip non-table values so the
                // scalar siblings are not misread as entries.
                if !entry_value.map(|v| v.is_table()).unwrap_or(false) {
                    continue;
                }
                if validate_identifier(k, &[]).is_err() {
                    tracing::warn!(
                        section = %section_path,
                        key = %k,
                        "skipping dynamic map entry with invalid identifier from disk",
                    );
                    continue;
                }
                entries.push(EntryState::build(
                    &section_path,
                    k.clone(),
                    entry_fields,
                    entry_value,
                ));
            }
        }
        let active_idx = if entries.is_empty() { None } else { Some(0) };
        Self {
            label,
            section_path,
            entry_fields,
            entries,
            active_idx,
            focus: MapFocus::SubtabStrip,
            add_modal: None,
            remove_modal: None,
            undo: UndoBuffer::new(),
            clock,
        }
    }

    pub fn entries(&self) -> &[EntryState] {
        &self.entries
    }

    pub fn active_entry(&self) -> Option<&EntryState> {
        self.active_idx.and_then(|i| self.entries.get(i))
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active_idx
    }

    pub fn focus(&self) -> &MapFocus {
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

        match key.code {
            KeyCode::Char('a') => {
                self.open_add_modal();
                WidgetAction::None
            }
            KeyCode::Char('d') if self.active_idx.is_some() => {
                self.open_remove_modal();
                WidgetAction::None
            }
            KeyCode::Char('u') => {
                self.attempt_undo();
                WidgetAction::None
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.next_tab();
                WidgetAction::None
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.prev_tab();
                WidgetAction::None
            }
            KeyCode::Down | KeyCode::Tab => {
                if matches!(self.focus, MapFocus::SubtabStrip) && self.active_entry().is_some() {
                    self.focus = MapFocus::EntryField(0);
                }
                WidgetAction::None
            }
            KeyCode::Up => {
                if let MapFocus::EntryField(0) = self.focus {
                    self.focus = MapFocus::SubtabStrip;
                } else if let MapFocus::EntryField(n) = self.focus {
                    self.focus = MapFocus::EntryField(n - 1);
                }
                WidgetAction::None
            }
            _ => {
                if let MapFocus::EntryField(n) = self.focus
                    && let Some(active) = self.active_idx
                    && let Some(entry) = self.entries.get_mut(active)
                    && let Some(field) = entry.fields.get_mut(n)
                {
                    return field.widget.handle_input(key);
                }
                WidgetAction::None
            }
        }
    }

    fn next_tab(&mut self) {
        if let Some(idx) = self.active_idx {
            let n = self.entries.len();
            if n > 0 {
                self.active_idx = Some((idx + 1) % n);
            }
        }
    }

    fn prev_tab(&mut self) {
        if let Some(idx) = self.active_idx {
            let n = self.entries.len();
            if n > 0 {
                self.active_idx = Some(if idx == 0 { n - 1 } else { idx - 1 });
            }
        }
    }

    fn open_add_modal(&mut self) {
        let existing: Vec<String> = self.entries.iter().map(|e| e.id.clone()).collect();
        self.add_modal = Some(AddEntryModal::new(
            format!("Add {} entry", self.section_path),
            existing,
        ));
        self.focus = MapFocus::AddModal;
    }

    fn open_remove_modal(&mut self) {
        let label = self
            .active_entry()
            .map(|e| format!("{}.{}", self.section_path, e.id))
            .unwrap_or_default();
        self.remove_modal = Some(RemoveConfirmModal::new(label));
        self.focus = MapFocus::RemoveConfirm;
    }

    fn dispatch_add(&mut self, action: ModalAction) -> WidgetAction {
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

    fn dispatch_remove(&mut self, action: ModalAction) -> WidgetAction {
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

    fn attempt_undo(&mut self) {
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

    pub fn needs_insert_mode(&self) -> bool {
        if self.add_modal.is_some() {
            return true;
        }
        if self.remove_modal.is_some() {
            return false;
        }
        if let MapFocus::EntryField(n) = self.focus
            && let Some(entry) = self.active_entry()
            && let Some(field) = entry.fields.get(n)
        {
            return field.widget.needs_insert_mode();
        }
        false
    }

    pub fn edit_hint(&self) -> (&'static str, &'static str) {
        ("a/d/Ctrl←→", "Add/Del/Switch")
    }

    pub fn serialize_to_toml(&self) -> toml::Value {
        let mut t = toml::map::Map::new();
        for entry in &self.entries {
            t.insert(entry.id.clone(), entry.to_toml(self.entry_fields));
        }
        toml::Value::Table(t)
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        super::dynamic_map_draw::draw(self, f, area, theme, focused);
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
