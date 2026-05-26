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
use crate::tui::screens::settings::schema_tab::widgets::DynamicMapWidget;
use crate::tui::widgets::{Dropdown, ListEditor, NumberStepper, TextInput, Toggle, WidgetKind};

pub struct EntryState {
    pub id: String,
    pub fields: Vec<SettingsField>,
    /// Sub-table values for dynamic-kind entry fields (`Map` /
    /// `FlattenedMap` / `VecOfStruct`). These render as `TextInput`
    /// placeholders today (the nested editor is deferred to the #872
    /// PR-B follow-up). We capture the raw value at build-time and
    /// re-emit it in `to_toml_filtered` so `merge_flattened_map`'s
    /// wholesale rewrite of `[teams.<id>]` does not drop the
    /// untouched sub-table.
    passthrough: std::collections::BTreeMap<&'static str, toml::Value>,
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
        let mut passthrough = std::collections::BTreeMap::new();
        for fs in entry_fields {
            let label = label_for(section_path, &id, fs.key);
            let value = existing.and_then(|v| v.get(fs.key));
            if matches!(
                fs.kind,
                FieldKind::Map { .. }
                    | FieldKind::FlattenedMap { .. }
                    | FieldKind::VecOfStruct { .. }
            ) && let Some(v) = value
            {
                passthrough.insert(fs.key, v.clone());
            }
            fields.push(SettingsField {
                widget: build_widget(label, fs, value),
            });
        }
        Self {
            id,
            fields,
            passthrough,
        }
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
                // Real widget wins (DynamicMap / DynamicRows): the widget
                // owns its own writeback via `serialize_to_toml`. Skip the
                // empty table case so an empty nested editor does not emit
                // a bare `role_overrides = {}` header (#901 AC).
                match &sf.widget {
                    WidgetKind::DynamicMap(_) | WidgetKind::DynamicRows(_) => {
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
                    }
                    _ => {
                        // Defense-in-depth passthrough fallback for kinds
                        // whose widget layer has not yet been lifted
                        // (FlattenedMap / VecOfStruct inside entries —
                        // currently rendered as read-only TextInput).
                        if let Some(v) = self.passthrough.get(fs.key) {
                            table.insert(fs.key.to_string(), v.clone());
                        }
                    }
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
            // `serialize_to_toml`; the EntryState.passthrough fallback
            // remains as defense-in-depth for the FlattenedMap /
            // VecOfStruct kinds below whose editors are not yet lifted.
            // Section_path doubles as the label so `display_name_for`
            // ("…role_overrides" → "role") drives the inner Add modal title.
            WidgetKind::DynamicMap(DynamicMapWidget::new(
                label.clone(),
                label,
                entry_fields,
                value,
            ))
        }
        FieldKind::NestedTable(_)
        | FieldKind::FlattenedMap { .. }
        | FieldKind::VecOfStruct { .. } => {
            // FlattenedMap / VecOfStruct inside an entry remain
            // read-only placeholders for now — no schema field
            // exercises them today. Passthrough preserves round-trip.
            let summary = dynamic_kind_summary(value);
            WidgetKind::TextInput(TextInput::new(label, summary).with_read_only())
        }
    }
}

fn dynamic_kind_summary(value: Option<&toml::Value>) -> String {
    let count = value
        .and_then(|v| v.as_table())
        .map(|t| t.len())
        .unwrap_or(0);
    if count == 0 {
        "(empty — read-only, editor in #901)".to_string()
    } else {
        format!("({count} entries — read-only, editor in #901)")
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
