//! `DynamicMapWidget` — sub-tab strip + active entry's field group for
//! `FieldKind::Map`. Owns its entries, the optional Add/Remove modals, and
//! a single-slot undo buffer with a 5-second window.
//!
//! The state machine + handle_input dispatch live here. Helpers split into
//! `visibility`, `focus`, `modals`, and `sizing` submodules to keep this
//! file under the 400-LOC guardrail (RUST-GUARDRAILS §1).

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

use crate::config::schema::FieldSchema;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetAction;

use super::super::modals::add_entry::AddEntryModal;
use super::super::modals::remove_confirm::RemoveConfirmModal;
use super::clock::{Clock, SystemClock};
use super::entry_state::EntryState;
use super::identifier::validate_identifier;
use super::undo::UndoBuffer;

pub(super) mod focus;
pub(super) mod modals;
pub(super) mod sizing;
pub(super) mod visibility;

/// Display label used in modal titles and other user-facing strings.
/// The section_path is the TOML key path (e.g. `"agents"`); for the
/// sake of UX clarity the `[agents]` table is presented as "provider"
/// in the TUI and `*.role_overrides` is presented as "role". Other
/// section_paths fall back to themselves.
pub(super) fn display_name_for(section_path: &str) -> &str {
    if section_path.ends_with("agents") {
        "provider"
    } else if section_path.ends_with("role_overrides") {
        "role"
    } else {
        section_path
    }
}

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
    pub(super) entries: Vec<EntryState>,
    pub(super) active_idx: Option<usize>,
    pub(super) focus: MapFocus,
    pub(super) add_modal: Option<AddEntryModal>,
    pub(super) remove_modal: Option<RemoveConfirmModal>,
    pub(super) undo: UndoBuffer,
    pub(super) clock: Arc<dyn Clock>,
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

        // While an inner text-editing widget owns input, every key goes
        // straight to it — otherwise typing "opencode" into a String
        // field would route `d` to the Remove-entry shortcut below.
        if let MapFocus::EntryField(n) = self.focus
            && let Some(active) = self.active_idx
            && let Some(entry) = self.entries.get_mut(active)
            && let Some(field) = entry.fields.get_mut(n)
            && field.widget.needs_insert_mode()
        {
            return field.widget.handle_input(key);
        }

        // ListEditor owns its own `a` / `d` / Enter chords. Defer to the
        // inner widget when the focused field claims them so the outer
        // DynamicMap does not steal Add Entry / Remove Entry chords.
        if self.focused_field_owns_chord(key) {
            return self.delegate_to_focused_field(key);
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
            // `[` / `]` switch entries while focus is on the subtab strip.
            // Avoid them when an EntryField is focused so they remain
            // available as literals for inner text widgets.
            KeyCode::Char(']') if matches!(self.focus, MapFocus::SubtabStrip) => {
                self.next_tab();
                WidgetAction::None
            }
            KeyCode::Char('[') if matches!(self.focus, MapFocus::SubtabStrip) => {
                self.prev_tab();
                WidgetAction::None
            }
            KeyCode::Down | KeyCode::Tab => {
                // Cooperative: let a focused nested widget advance first.
                if self.try_advance_focused_inner(true) {
                    return WidgetAction::None;
                }
                self.focus_next_field();
                WidgetAction::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                if self.try_advance_focused_inner(false) {
                    return WidgetAction::None;
                }
                self.focus_prev_field();
                WidgetAction::None
            }
            _ => {
                if let MapFocus::EntryField(n) = self.focus
                    && let Some(active) = self.active_idx
                    && let Some(entry) = self.entries.get_mut(active)
                    && let Some(field) = entry.fields.get_mut(n)
                {
                    let action = field.widget.handle_input(key);
                    // Changing `kind` can hide the currently focused field
                    // (e.g. switching to `ollama` hides `command`). Snap
                    // focus to the nearest visible field so the cursor
                    // never sits on a row the user can't see.
                    self.clamp_focus_to_visible();
                    return action;
                }
                WidgetAction::None
            }
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

    pub fn edit_hint(&self) -> &'static [(&'static str, &'static str)] {
        // When focus is on an entry-field whose widget owns its own chords
        // (ListEditor `a/d/Enter`, TextInput `Enter`, …), surface that
        // widget's hint instead of the outer Add/Del/Prev/Next chords —
        // otherwise the bar misleads users editing a `bindings` /
        // `extra_args` ListEditor into pressing `a` for the wrong modal.
        if let MapFocus::EntryField(n) = self.focus
            && let Some(active) = self.active_idx
            && let Some(entry) = self.entries.get(active)
            && let Some(field) = entry.fields.get(n)
        {
            return field.widget.edit_hint();
        }
        &[("a/d", "Add/Del"), ("[ ]", "Prev/Next")]
    }

    pub fn serialize_to_toml(&self) -> toml::Value {
        let mut t = toml::map::Map::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let visible = self.visible_field_indices_for(i);
            t.insert(
                entry.id.clone(),
                entry.to_toml_filtered(self.entry_fields, &visible),
            );
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
