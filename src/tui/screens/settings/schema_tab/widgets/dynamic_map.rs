//! `DynamicMapWidget` — sub-tab strip + active entry's field group for
//! `FieldKind::Map`. Owns its entries, the optional Add/Remove modals, and
//! a single-slot undo buffer with a 5-second window.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

use crate::config::schema::FieldSchema;
use crate::config::schema::dynamic::agent_field_visible_for_kind;
use crate::tui::theme::Theme;
use crate::tui::widgets::{WidgetAction, WidgetKind};

/// Display label used in modal titles and other user-facing strings.
/// The section_path is the TOML key path (e.g. `"agents"`); for the
/// sake of UX clarity the `[agents]` table is presented as "provider"
/// in the TUI. Other section_paths fall back to themselves.
fn display_name_for(section_path: &str) -> &str {
    if section_path.ends_with("agents") {
        "provider"
    } else {
        section_path
    }
}

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

        // ListEditor owns its own `a` / `d` / Enter chords (Add / Delete /
        // Edit list item). When focus is on a ListEditor entry field, defer
        // these keys to the inner widget so users can edit `bindings` /
        // `extra_args` / `allowed_tools` lists — otherwise the outer
        // DynamicMap eats `a`/`d` for its own Add Entry / Remove Entry
        // modals.
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
            // We avoid them when an EntryField is focused so they remain
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
                self.focus_next_field();
                WidgetAction::None
            }
            KeyCode::Up | KeyCode::BackTab => {
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

    /// Subset of `entry_fields` indices that should render and accept
    /// focus for the currently active entry. Convenience wrapper over
    /// [`Self::visible_field_indices_for`] using `self.active_idx`.
    pub(super) fn visible_field_indices(&self) -> Vec<usize> {
        let active = self.active_idx.unwrap_or(0);
        self.visible_field_indices_for(active)
    }

    /// Subset of `entry_fields` indices visible for the entry at
    /// `entry_idx`. For `[agents.<id>]` this hides kind-incompatible
    /// rows (e.g. base_url when kind is a subprocess agent). All
    /// other section_paths fall through to the full index list.
    pub(super) fn visible_field_indices_for(&self, entry_idx: usize) -> Vec<usize> {
        let all: Vec<usize> = (0..self.entry_fields.len()).collect();
        if !self.section_path.ends_with("agents") {
            return all;
        }
        let Some(entry) = self.entries.get(entry_idx) else {
            return all;
        };
        let kind_value = entry
            .fields
            .iter()
            .zip(self.entry_fields.iter())
            .find(|(_, fs)| fs.key == "kind")
            .and_then(|(sf, _)| match &sf.widget {
                WidgetKind::Dropdown(d) => Some(d.selected_value().to_string()),
                WidgetKind::TextInput(t) => Some(t.value.clone()),
                _ => None,
            })
            .unwrap_or_default();
        self.entry_fields
            .iter()
            .enumerate()
            .filter(|(_, fs)| agent_field_visible_for_kind(fs.key, &kind_value))
            .map(|(i, _)| i)
            .collect()
    }

    /// If focus is on an EntryField that is no longer in the visible set
    /// (e.g. user switched `kind` away from the one that included it),
    /// snap to the nearest visible field — preferring the next one, then
    /// the previous, then back to SubtabStrip if nothing remains.
    fn clamp_focus_to_visible(&mut self) {
        if let MapFocus::EntryField(n) = self.focus {
            let visible = self.visible_field_indices();
            if visible.contains(&n) {
                return;
            }
            if let Some(next) = visible.iter().find(|&&i| i > n).copied() {
                self.focus = MapFocus::EntryField(next);
            } else if let Some(prev) = visible.iter().rev().find(|&&i| i < n).copied() {
                self.focus = MapFocus::EntryField(prev);
            } else {
                self.focus = MapFocus::SubtabStrip;
            }
        }
    }

    /// Walk inner focus one step backward without consuming the outer
    /// SettingsScreen Up arrow. Returns true when focus moved so the
    /// outer can leave `field_index` alone — only a return of `false`
    /// signals the boundary (`SubtabStrip`) and lets the outer cursor
    /// climb up to the previous field.
    pub fn try_focus_prev(&mut self) -> bool {
        if !matches!(self.focus, MapFocus::EntryField(_)) {
            return false;
        }
        let before = self.focus.clone();
        self.focus_prev_field();
        self.focus != before
    }

    /// Mirror of [`try_focus_prev`] for the Down arrow. Returns true when
    /// focus advanced into (or further inside) the entry-field group.
    pub fn try_focus_next(&mut self) -> bool {
        match self.focus {
            MapFocus::SubtabStrip if self.active_entry().is_some() => {
                let before = self.focus.clone();
                self.focus_next_field();
                self.focus != before
            }
            MapFocus::EntryField(_) => {
                let before = self.focus.clone();
                self.focus_next_field();
                self.focus != before
            }
            _ => false,
        }
    }

    fn focus_next_field(&mut self) {
        let visible = self.visible_field_indices();
        match self.focus {
            MapFocus::SubtabStrip if self.active_entry().is_some() => {
                if let Some(&first) = visible.first() {
                    self.focus = MapFocus::EntryField(first);
                }
            }
            MapFocus::EntryField(n) => {
                if let Some(next) = visible.iter().find(|&&idx| idx > n).copied() {
                    self.focus = MapFocus::EntryField(next);
                }
            }
            _ => {}
        }
    }

    fn focus_prev_field(&mut self) {
        let visible = self.visible_field_indices();
        if let MapFocus::EntryField(n) = self.focus {
            let prev = visible.iter().rev().find(|&&idx| idx < n).copied();
            match prev {
                Some(idx) => self.focus = MapFocus::EntryField(idx),
                None => self.focus = MapFocus::SubtabStrip,
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
        let display_name = display_name_for(&self.section_path);
        self.add_modal = Some(AddEntryModal::new(
            format!("Add {} entry", display_name),
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

    /// Total vertical lines the widget would like to occupy when rendered.
    /// Used by the settings screen's `field_height` so multi-line per-entry
    /// fields (notably `ListEditor` empty-state hints and edit prompts) are
    /// not clipped to a single row.
    ///
    /// Header (1) + tabstrip (2) + sum of per-row heights when entries
    /// exist; the empty-state hint takes 4 lines (header + "No entries" +
    /// blank + `[a] add first entry`).
    pub fn desired_height(&self) -> u16 {
        let header = 1u16;
        if self.entries.is_empty() {
            return header + 3;
        }
        let tabstrip = 2u16;
        let body = self.active_entry_body_height();
        header + tabstrip + body
    }

    fn active_entry_body_height(&self) -> u16 {
        let Some(active) = self.active_idx else {
            return 0;
        };
        let Some(entry) = self.entries.get(active) else {
            return 0;
        };
        let visible = self.visible_field_indices();
        visible
            .iter()
            .map(|&idx| {
                let focused = matches!(self.focus, MapFocus::EntryField(n) if n == idx);
                entry
                    .fields
                    .get(idx)
                    .map(|f| entry_row_height(&f.widget, focused))
                    .unwrap_or(1)
            })
            .sum()
    }

    /// Per-field row heights for the currently active entry, paired with
    /// the field-index in `entry_fields`. Used by `dynamic_map_draw` to
    /// build the per-row layout constraints.
    pub(super) fn active_entry_row_heights(&self) -> Vec<(usize, u16)> {
        let Some(active) = self.active_idx else {
            return Vec::new();
        };
        let Some(entry) = self.entries.get(active) else {
            return Vec::new();
        };
        self.visible_field_indices()
            .into_iter()
            .map(|idx| {
                let focused = matches!(self.focus, MapFocus::EntryField(n) if n == idx);
                let h = entry
                    .fields
                    .get(idx)
                    .map(|f| entry_row_height(&f.widget, focused))
                    .unwrap_or(1);
                (idx, h)
            })
            .collect()
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

    /// True when the focused entry-field widget claims `key` as one of its
    /// own chords. Today only `ListEditor` does (it owns `a`, `d`, and
    /// `Enter` in its non-editing mode). Other widget kinds either ignore
    /// these keys or only react to them while in insert mode (handled by
    /// the `needs_insert_mode()` gate above).
    fn focused_field_owns_chord(&self, key: KeyEvent) -> bool {
        let MapFocus::EntryField(n) = self.focus else {
            return false;
        };
        let Some(active) = self.active_idx else {
            return false;
        };
        let Some(entry) = self.entries.get(active) else {
            return false;
        };
        let Some(field) = entry.fields.get(n) else {
            return false;
        };
        match field.widget {
            WidgetKind::ListEditor(_) => matches!(
                key.code,
                KeyCode::Char('a') | KeyCode::Char('d') | KeyCode::Enter
            ),
            _ => false,
        }
    }

    fn delegate_to_focused_field(&mut self, key: KeyEvent) -> WidgetAction {
        let MapFocus::EntryField(n) = self.focus else {
            return WidgetAction::None;
        };
        let Some(active) = self.active_idx else {
            return WidgetAction::None;
        };
        let Some(entry) = self.entries.get_mut(active) else {
            return WidgetAction::None;
        };
        let Some(field) = entry.fields.get_mut(n) else {
            return WidgetAction::None;
        };
        let action = field.widget.handle_input(key);
        self.clamp_focus_to_visible();
        action
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

/// Vertical lines a single per-entry field row needs. `ListEditor` is the
/// only widget that ever exceeds 1 line — it draws an `[a]/[d]` hint when
/// focused-empty, an input prompt when editing, and one line per item.
/// All other widgets render in a single line.
///
/// Caps item rendering at `MAX_LIST_ROWS` so an entry with hundreds of
/// items doesn't push other fields off-screen; long lists scroll inside
/// the widget (deferred to a follow-up).
fn entry_row_height(widget: &WidgetKind, focused: bool) -> u16 {
    const MAX_LIST_ROWS: u16 = 4;
    match widget {
        WidgetKind::ListEditor(le) => {
            let items = (le.items.len() as u16).min(MAX_LIST_ROWS);
            if le.editing {
                // label + items (capped) + input prompt
                1 + items + 1
            } else if focused {
                if le.items.is_empty() {
                    // label + `[a] Add  [d] Delete` hint
                    2
                } else {
                    // label + items + hint
                    1 + items + 1
                }
            } else if items == 0 {
                1
            } else {
                1 + items
            }
        }
        _ => 1,
    }
}
