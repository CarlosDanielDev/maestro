//! Vertical-layout sizing for [`super::DynamicMapWidget`].
//!
//! Split out of `dynamic_map.rs` (#908). Owns `desired_height`,
//! `active_entry_row_heights_for`, and the per-widget row-height table
//! used by `dynamic_map_draw` to build layout constraints.

use std::collections::HashMap;

use crate::tui::screens::settings::validation::ValidationFeedback;
use crate::tui::widgets::WidgetKind;

use super::{DynamicMapWidget, MapFocus};

impl DynamicMapWidget {
    /// Total vertical lines the widget would like to occupy when rendered.
    /// Used by the settings screen's `field_height` so multi-line per-entry
    /// fields (notably `ListEditor` empty-state hints and edit prompts) are
    /// not clipped to a single row.
    ///
    /// Header (1) + tabstrip (2) + sum of per-row heights when entries
    /// exist; the empty-state hint takes 4 lines (header + "No entries" +
    /// blank + `[a] add first entry`).
    pub fn desired_height(&self) -> u16 {
        let empty: HashMap<String, ValidationFeedback> = HashMap::new();
        self.desired_height_with_warnings(&empty)
    }

    /// Variant of [`Self::desired_height`] that grows the body line
    /// count when sub-fields have pending warnings so the outer
    /// [`SettingsScreen::field_height`] still allocates enough room
    /// for the inline-warning lines (#909).
    pub fn desired_height_with_warnings(
        &self,
        warnings: &HashMap<String, ValidationFeedback>,
    ) -> u16 {
        let header = 1u16;
        if self.entries.is_empty() {
            return header + 3;
        }
        let tabstrip = 2u16;
        let body = self.active_entry_body_height_for(warnings);
        header + tabstrip + body
    }

    fn active_entry_body_height_for(&self, warnings: &HashMap<String, ValidationFeedback>) -> u16 {
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
                    .map(|f| match &f.widget {
                        // Nested DynamicMap defers to its own warning-
                        // aware desired_height so the outer body still
                        // accounts for a child sub-field's inline
                        // warning line.
                        WidgetKind::DynamicMap(inner) if focused => {
                            inner.desired_height_with_warnings(warnings)
                        }
                        other => {
                            let has_warning = warnings
                                .get(other.label())
                                .is_some_and(|fb| !fb.message.is_empty());
                            entry_row_height_with_warning(other, focused, has_warning)
                        }
                    })
                    .unwrap_or(1)
            })
            .sum()
    }

    /// Per-field row heights for the currently active entry, paired
    /// with the field-index in `entry_fields`. Consults a
    /// warnings-by-label lookup so a sub-field with a pending
    /// `ValidationFeedback::warning` is allocated a second line for
    /// the inline message (TextInput renders the warning at `y + 1`
    /// and silently drops it when `area.height <= 1`). The map is
    /// the same one threaded into `draw_with_warnings` (#909);
    /// callers with no warnings pass `&HashMap::new()`.
    pub(in crate::tui::screens::settings::schema_tab::widgets) fn active_entry_row_heights_for(
        &self,
        warnings: &HashMap<String, ValidationFeedback>,
    ) -> Vec<(usize, u16)> {
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
                    .map(|f| match &f.widget {
                        WidgetKind::DynamicMap(inner) if focused => {
                            inner.desired_height_with_warnings(warnings)
                        }
                        other => {
                            let has_warning = warnings
                                .get(other.label())
                                .is_some_and(|fb| !fb.message.is_empty());
                            entry_row_height_with_warning(other, focused, has_warning)
                        }
                    })
                    .unwrap_or(1);
                (idx, h)
            })
            .collect()
    }
}

/// Vertical lines a single per-entry field row needs. `ListEditor` is
/// the only widget that ever exceeds 1 line — it draws an `[a]/[d]` hint
/// when focused-empty, an input prompt when editing, and one line per
/// item. All other single-line widgets bump to 2 lines when
/// `has_warning` is true so `TextInput::draw` can paint its inline
/// `ValidationFeedback` message on the line below the field (#909).
///
/// Caps item rendering at `MAX_LIST_ROWS` so an entry with hundreds of
/// items doesn't push other fields off-screen; long lists scroll inside
/// the widget (deferred to a follow-up).
fn entry_row_height_with_warning(widget: &WidgetKind, focused: bool, has_warning: bool) -> u16 {
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
        // Nested DynamicMap (e.g. the role_overrides editor inside a
        // team entry) owns its own desired_height — header + tabstrip +
        // per-role-field rows. Defer to it so the outer layout gives
        // the nested editor the rows it needs to render legibly.
        WidgetKind::DynamicMap(inner) if focused => inner.desired_height(),
        _ => {
            if has_warning {
                2
            } else {
                1
            }
        }
    }
}
