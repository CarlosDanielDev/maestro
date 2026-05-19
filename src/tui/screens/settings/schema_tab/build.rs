//! Build `SettingsField` entries from a `TableSchema`.
//!
//! The renderer maps each `FieldKind` to a `WidgetKind`. Fields not present
//! in the serialized `Config` fall back to `field.default` — tests rely on
//! this to inject synthetic schemas that do not appear in the production
//! registry. `Float` fields are rendered through `NumberStepper` and lose
//! sub-integer precision; tabs that need fractional steps must keep the
//! hand-coded path until a dedicated float widget exists.

use crate::config::Config;
use crate::config::schema::{DefaultValue, FieldKind, FieldSchema, TableSchema};
use crate::tui::screens::settings::SettingsField;
use crate::tui::widgets::{Dropdown, ListEditor, NumberStepper, TextInput, Toggle, WidgetKind};

/// Build a flat `Vec<SettingsField>` from a `TableSchema`.
/// `FieldKind::NestedTable(inner)` is flattened: each inner field is emitted
/// with label `"<outer_key>.<inner_key>"`.
pub(crate) fn from_schema(table: &'static TableSchema, config: &Config) -> Vec<SettingsField> {
    let root = match toml::Value::try_from(config) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                table = table.name,
                "schema_tab: serializing Config to toml::Value failed; using defaults",
            );
            toml::Value::Table(toml::map::Map::new())
        }
    };

    let mut out = Vec::new();
    for field in table.fields {
        push_field(&mut out, table.name, &[], field, &root);
    }
    out
}

fn push_field(
    out: &mut Vec<SettingsField>,
    table_name: &str,
    prefix: &[&str],
    field: &FieldSchema,
    root: &toml::Value,
) {
    if let FieldKind::NestedTable(inner) = field.kind {
        let mut next_prefix: Vec<&str> = prefix.to_vec();
        next_prefix.push(field.key);
        for child in inner {
            push_field(out, table_name, &next_prefix, child, root);
        }
        return;
    }

    let label = label_for(prefix, field.key);
    let value = resolve_field_value(table_name, prefix, field.key, root);
    let widget = widget_for_kind(label, field, value.as_ref());
    out.push(SettingsField { widget });
}

fn label_for(prefix: &[&str], key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        let mut joined = prefix.join(".");
        joined.push('.');
        joined.push_str(key);
        joined
    }
}

fn resolve_field_value(
    table_name: &str,
    prefix: &[&str],
    key: &str,
    root: &toml::Value,
) -> Option<toml::Value> {
    let mut node = root;
    for segment in table_name.split('.') {
        node = node.get(segment)?;
    }
    for segment in prefix {
        node = node.get(*segment)?;
    }
    node.get(key).cloned()
}

fn widget_for_kind(label: String, field: &FieldSchema, value: Option<&toml::Value>) -> WidgetKind {
    match field.kind {
        FieldKind::Bool => {
            let v = value
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| default_bool(field.default));
            WidgetKind::Toggle(Toggle::new(label, v))
        }
        FieldKind::Int { min, max, step } => {
            let v = value
                .and_then(|v| v.as_integer())
                .unwrap_or_else(|| default_int(field.default));
            WidgetKind::NumberStepper(NumberStepper::new(label, v, min, max).with_step(step))
        }
        FieldKind::Float { min, max, step } => {
            let raw = value
                .and_then(|v| v.as_float())
                .unwrap_or_else(|| default_float(field.default));
            let v = raw.round() as i64;
            let min_i = (min.floor() as i64).min(v);
            let max_i = (max.ceil() as i64).max(v);
            let step_i = (step.ceil() as i64).max(1);
            WidgetKind::NumberStepper(NumberStepper::new(label, v, min_i, max_i).with_step(step_i))
        }
        FieldKind::String => {
            let v = value
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| default_str(field.default).to_string());
            WidgetKind::TextInput(TextInput::new(label, v))
        }
        FieldKind::Enum(variants) => {
            let current = value
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| default_str(field.default).to_string());
            let idx = variants
                .iter()
                .position(|v| *v == current.as_str())
                .unwrap_or(0);
            let options: Vec<String> = variants.iter().map(|s| (*s).to_string()).collect();
            WidgetKind::Dropdown(Dropdown::new(label, options, idx))
        }
        FieldKind::StringList => {
            let items: Vec<String> = value
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_else(|| {
                    default_str_list(field.default)
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect()
                });
            WidgetKind::ListEditor(ListEditor::new(label, items))
        }
        FieldKind::NestedTable(_) => unreachable!("NestedTable handled by push_field"),
    }
}

fn default_bool(default: DefaultValue) -> bool {
    match default {
        DefaultValue::Bool(v) => v,
        _ => false,
    }
}

fn default_int(default: DefaultValue) -> i64 {
    match default {
        DefaultValue::Int(v) => v,
        _ => 0,
    }
}

fn default_float(default: DefaultValue) -> f64 {
    match default {
        DefaultValue::Float(v) => v,
        _ => 0.0,
    }
}

fn default_str(default: DefaultValue) -> &'static str {
    match default {
        DefaultValue::Str(v) => v,
        _ => "",
    }
}

fn default_str_list(default: DefaultValue) -> &'static [&'static str] {
    match default {
        DefaultValue::StrList(v) => v,
        _ => &[],
    }
}
