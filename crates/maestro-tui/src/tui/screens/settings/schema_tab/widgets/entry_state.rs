//! Per-entry materialized field group for `DynamicMapWidget` and
//! `DynamicRowsWidget`.
//!
//! An `EntryState` wraps the identifier of a dynamic entry plus the
//! `SettingsField` list built from `FieldSchema::entry_fields`. Labels are
//! namespaced `<section_path>.<id>.<key>` so they do not collide with
//! static field labels in the same tab.

use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::teams_bindings::collapse_team_bindings_into_array;
use crate::tui::screens::settings::schema_tab::widgets::{DynamicMapWidget, DynamicRowsWidget};
use crate::tui::widgets::{Dropdown, ListEditor, NumberStepper, TextInput, Toggle, WidgetKind};

pub struct EntryState {
    pub id: String,
    pub fields: Vec<SettingsField>,
}

impl EntryState {
    pub fn build(
        section_path: &str,
        id: impl Into<String>,
        entry_fields: &'static [FieldSchema],
        existing: Option<&toml::Value>,
    ) -> Self {
        let id = id.into();
        // Section-specific reshape: `[teams.<id>]` stores bindings as
        // top-level scalar keys on the entry table (`#[serde(flatten)]`).
        // Fold them into a synthetic `bindings = [...]` array so the
        // `StringList` builder finds the field where it expects it. No-op
        // for every other section.
        let collapsed = collapse_team_bindings_into_array(section_path, existing);
        let existing = collapsed.as_ref().or(existing);
        let mut fields = Vec::with_capacity(entry_fields.len());
        for fs in entry_fields {
            let label = label_for(section_path, &id, fs.key);
            let value = existing.and_then(|v| v.get(fs.key));
            fields.push(SettingsField {
                widget: build_widget(label, fs, value),
            });
        }
        Self { id, fields }
    }

    pub fn label_for(&self, section_path: &str, key: &str) -> String {
        label_for(section_path, &self.id, key)
    }

    pub fn to_toml(&self, entry_fields: &'static [FieldSchema]) -> toml::Value {
        let all: Vec<usize> = (0..entry_fields.len()).collect();
        self.to_toml_filtered(entry_fields, &all)
    }

    /// Serialize the entry table, including only the field indices in
    /// `visible_indices`. Used by `DynamicMap.serialize_to_toml` to drop
    /// kind-incompatible agent fields from the saved TOML so the
    /// validator never sees stale HTTP-era values on a subprocess
    /// agent (and vice-versa).
    pub fn to_toml_filtered(
        &self,
        entry_fields: &'static [FieldSchema],
        visible_indices: &[usize],
    ) -> toml::Value {
        let mut table = toml::map::Map::new();
        for &idx in visible_indices {
            let Some(fs) = entry_fields.get(idx) else {
                continue;
            };
            let Some(sf) = self.fields.get(idx) else {
                continue;
            };
            if matches!(
                fs.kind,
                FieldKind::Map { .. }
                    | FieldKind::FlattenedMap { .. }
                    | FieldKind::VecOfStruct { .. }
            ) {
                // Every dynamic-kind entry-field now owns a live widget
                // (DynamicMap or DynamicRows) — see `build_widget` below.
                // Skip the empty table/array case so an empty nested
                // editor does not emit a bare `role_overrides = {}`
                // header (#901 / #908 AC).
                let v = widget_value(&sf.widget);
                let omit_empty = matches!(
                    &v,
                    toml::Value::Table(t) if t.is_empty()
                ) || matches!(
                    &v,
                    toml::Value::Array(a) if a.is_empty()
                );
                if !omit_empty {
                    table.insert(fs.key.to_string(), v);
                }
                continue;
            }
            table.insert(fs.key.to_string(), widget_value(&sf.widget));
        }
        toml::Value::Table(table)
    }
}

fn label_for(section_path: &str, id: &str, key: &str) -> String {
    format!("{}.{}.{}", section_path, id, key)
}

fn build_widget(label: String, fs: &FieldSchema, value: Option<&toml::Value>) -> WidgetKind {
    match fs.kind {
        FieldKind::Bool => {
            let default_v = matches!(fs.default, DefaultValue::Bool(true));
            let v = value.and_then(|v| v.as_bool()).unwrap_or(default_v);
            WidgetKind::Toggle(Toggle::new(label, v))
        }
        FieldKind::Int { min, max, step } => {
            let default_v = match fs.default {
                DefaultValue::Int(n) => n,
                _ => 0,
            };
            let v = value.and_then(|v| v.as_integer()).unwrap_or(default_v);
            WidgetKind::NumberStepper(NumberStepper::new(label, v, min, max).with_step(step))
        }
        FieldKind::Float { .. } => {
            // Dynamic entry float fields fall back to integer rendering for
            // simplicity; #791 scope does not register Float entry fields.
            WidgetKind::NumberStepper(NumberStepper::new(label, 0, 0, 0))
        }
        FieldKind::String => {
            let v = value
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| default_str(fs).to_string());
            WidgetKind::TextInput(TextInput::new(label, v))
        }
        FieldKind::Enum(variants) => {
            let current = value
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| default_str(fs).to_string());
            let idx = variants
                .iter()
                .position(|v| *v == current.as_str())
                .unwrap_or(0);
            let opts: Vec<String> = variants.iter().map(|s| (*s).to_string()).collect();
            WidgetKind::Dropdown(Dropdown::new(label, opts, idx))
        }
        FieldKind::StringList => {
            let items: Vec<String> = value
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            WidgetKind::ListEditor(ListEditor::new(label, items))
        }
        FieldKind::Map { entry_fields } => {
            // Nested DynamicMap inside an entry (e.g. role_overrides
            // inside a team). The widget owns the writeback path via
            // `serialize_to_toml`. Section_path doubles as the label so
            // `display_name_for` ("…role_overrides" → "role") drives
            // the inner Add modal title.
            WidgetKind::DynamicMap(DynamicMapWidget::new(
                label.clone(),
                label,
                entry_fields,
                value,
            ))
        }
        FieldKind::FlattenedMap { entry_fields } => {
            // #908 — FlattenedMap inside an entry now produces a live
            // DynamicMap widget. The widget owns writeback via
            // `serialize_to_toml`; empty omission is handled by
            // `to_toml_filtered` above.
            WidgetKind::DynamicMap(DynamicMapWidget::new(
                label.clone(),
                label,
                entry_fields,
                value,
            ))
        }
        FieldKind::VecOfStruct { entry_fields } => {
            // #908 — VecOfStruct inside an entry now produces a live
            // DynamicRows widget.
            WidgetKind::DynamicRows(DynamicRowsWidget::new(
                label.clone(),
                label,
                entry_fields,
                value,
            ))
        }
        FieldKind::NestedTable(_) => {
            // NestedTable inside an entry stays a read-only placeholder
            // — no schema field exercises it today and lifting it
            // requires schema-walker design work tracked separately.
            WidgetKind::TextInput(TextInput::new(label, String::new()).with_read_only())
        }
    }
}

fn default_str(fs: &FieldSchema) -> &'static str {
    match fs.default {
        DefaultValue::Str(s) => s,
        _ => "",
    }
}

fn widget_value(widget: &WidgetKind) -> toml::Value {
    match widget {
        WidgetKind::Toggle(w) => toml::Value::Boolean(w.value),
        WidgetKind::TextInput(w) => toml::Value::String(w.value.clone()),
        WidgetKind::NumberStepper(w) => toml::Value::Integer(w.value),
        WidgetKind::Dropdown(w) => toml::Value::String(w.selected_value().to_string()),
        WidgetKind::ListEditor(w) => {
            toml::Value::Array(w.items.iter().cloned().map(toml::Value::String).collect())
        }
        WidgetKind::DynamicMap(w) => w.serialize_to_toml(),
        WidgetKind::DynamicRows(w) => w.serialize_to_toml(),
    }
}
