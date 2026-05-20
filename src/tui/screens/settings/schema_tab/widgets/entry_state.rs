//! Per-entry materialized field group for `DynamicMapWidget` and
//! `DynamicRowsWidget`.
//!
//! An `EntryState` wraps the identifier of a dynamic entry plus the
//! `SettingsField` list built from `FieldSchema::entry_fields`. Labels are
//! namespaced `<section_path>.<id>.<key>` so they do not collide with
//! static field labels in the same tab.

use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
use crate::tui::screens::settings::SettingsField;
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
        FieldKind::NestedTable(_)
        | FieldKind::Map { .. }
        | FieldKind::FlattenedMap { .. }
        | FieldKind::VecOfStruct { .. } => {
            // Nested dynamic shapes inside entries are out of scope for #791.
            WidgetKind::TextInput(TextInput::new(label, ""))
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
        WidgetKind::DynamicMap(_) | WidgetKind::DynamicRows(_) => {
            toml::Value::Table(Default::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::settings::schema_tab::widgets::test_fixture::TEST_AGENT_FIELDS;

    #[test]
    fn builds_namespaced_labels() {
        let e = EntryState::build("agents", "qwen-fast", TEST_AGENT_FIELDS, None);
        assert_eq!(e.id, "qwen-fast");
        assert_eq!(e.fields.len(), TEST_AGENT_FIELDS.len());
        assert_eq!(e.fields[0].widget.label(), "agents.qwen-fast.kind");
        assert_eq!(e.fields[1].widget.label(), "agents.qwen-fast.enabled");
        assert_eq!(e.fields[2].widget.label(), "agents.qwen-fast.model");
    }

    #[test]
    fn label_for_assembles_dotted_path() {
        let e = EntryState::build("agents", "claude", TEST_AGENT_FIELDS, None);
        assert_eq!(e.label_for("agents", "kind"), "agents.claude.kind");
    }

    #[test]
    fn applies_defaults_when_no_existing_value() {
        let e = EntryState::build("agents", "claude", TEST_AGENT_FIELDS, None);
        if let WidgetKind::Dropdown(d) = &e.fields[0].widget {
            assert_eq!(d.selected_value(), "implementer");
        } else {
            panic!("expected dropdown for kind");
        }
    }

    #[test]
    fn applies_existing_values_from_toml() {
        let mut t = toml::map::Map::new();
        t.insert("kind".into(), toml::Value::String("reviewer".into()));
        t.insert("enabled".into(), toml::Value::Boolean(false));
        let value = toml::Value::Table(t);
        let e = EntryState::build("agents", "claude", TEST_AGENT_FIELDS, Some(&value));
        if let WidgetKind::Dropdown(d) = &e.fields[0].widget {
            assert_eq!(d.selected_value(), "reviewer");
        } else {
            panic!();
        }
        if let WidgetKind::Toggle(tg) = &e.fields[1].widget {
            assert!(!tg.value);
        } else {
            panic!();
        }
    }

    #[test]
    fn serializes_back_to_toml_table() {
        let e = EntryState::build("agents", "claude", TEST_AGENT_FIELDS, None);
        let v = e.to_toml(TEST_AGENT_FIELDS);
        let t = v.as_table().expect("table");
        assert_eq!(t.get("kind").and_then(|v| v.as_str()), Some("implementer"));
        assert!(t.get("enabled").and_then(|v| v.as_bool()).is_some());
    }
}
