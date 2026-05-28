//! Vertical-layout sizing for [`super::DynamicMapWidget`].
//!
//! Split out of `dynamic_map.rs` (#908). Owns `desired_height`,
//! `active_entry_row_heights`, and the per-widget row-height table used
//! by `dynamic_map_draw` to build layout constraints.

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
    pub(in crate::tui::screens::settings::schema_tab::widgets) fn active_entry_row_heights(
        &self,
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
                    .map(|f| entry_row_height(&f.widget, focused))
                    .unwrap_or(1);
                (idx, h)
            })
            .collect()
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
        // Nested DynamicMap (e.g. the role_overrides editor inside a
        // team entry) owns its own desired_height — header + tabstrip +
        // per-role-field rows. Defer to it so the outer layout gives
        // the nested editor the rows it needs to render legibly.
        WidgetKind::DynamicMap(inner) if focused => inner.desired_height(),
        _ => 1,
    }
}
