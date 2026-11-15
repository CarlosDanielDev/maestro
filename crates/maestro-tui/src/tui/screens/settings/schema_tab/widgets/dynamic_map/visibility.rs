//! Kind-aware field visibility helpers for [`super::DynamicMapWidget`].
//!
//! Split out of `dynamic_map.rs` (#908) so the state machine stays under
//! the 400-LOC guardrail. `pub(super)` because only the parent module
//! consumes these.

use crate::config::schema::dynamic::agent_field_visible_for_kind;
use crate::tui::widgets::WidgetKind;

use super::{DynamicMapWidget, MapFocus};

impl DynamicMapWidget {
    /// Subset of `entry_fields` indices that should render and accept
    /// focus for the currently active entry. Convenience wrapper over
    /// [`Self::visible_field_indices_for`] using `self.active_idx`.
    pub(in crate::tui::screens::settings::schema_tab::widgets) fn visible_field_indices(
        &self,
    ) -> Vec<usize> {
        let active = self.active_idx.unwrap_or(0);
        self.visible_field_indices_for(active)
    }

    /// Subset of `entry_fields` indices visible for the entry at
    /// `entry_idx'. For `[agents.<id>]` this hides kind-incompatible
    /// rows (e.g. base_url when kind is a subprocess agent). All
    /// other section_paths fall through to the full index list.
    pub(in crate::tui::screens::settings::schema_tab::widgets) fn visible_field_indices_for(
        &self,
        entry_idx: usize,
    ) -> Vec<usize> {
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
    pub(super) fn clamp_focus_to_visible(&mut self) {
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
}
