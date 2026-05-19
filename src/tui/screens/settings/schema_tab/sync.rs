//! Write widget values back into `Config` via schema walking.
//!
//! The public entry point is `sync_to_config`. It serializes `Config` into a
//! `toml::Value`, calls `apply_to_toml` to mutate fields keyed by the
//! schema, then deserializes the mutated tree back into `Config`. The
//! middle layer is exposed for testing — synthetic schemas that have no
//! Config home can exercise the writeback path without round-tripping.

use crate::config::Config;
use crate::config::schema::{FieldKind, FieldSchema, TableSchema};
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::validation::ValidationFeedback;
use crate::tui::widgets::WidgetKind;

/// Round-trip a schema-driven tab back into `Config`. Returns an error if
/// the mutated TOML cannot deserialize into the strongly-typed `Config`.
pub(crate) fn sync_to_config(
    table: &'static TableSchema,
    fields: &[SettingsField],
    config: &mut Config,
) -> anyhow::Result<()> {
    let mut root = toml::Value::try_from(&*config)?;
    apply_to_toml(table, fields, &mut root)?;
    *config = root.try_into()?;
    Ok(())
}

/// Mutate `root` in place by writing each leaf field's widget value into
/// the path `<table.name>.<...prefix>.<field.key>`.
pub(crate) fn apply_to_toml(
    table: &'static TableSchema,
    fields: &[SettingsField],
    root: &mut toml::Value,
) -> anyhow::Result<()> {
    for field in table.fields {
        write_field(table.name, &[], field, fields, root)?;
    }
    Ok(())
}

fn write_field(
    table_name: &str,
    prefix: &[&str],
    field: &FieldSchema,
    fields: &[SettingsField],
    root: &mut toml::Value,
) -> anyhow::Result<()> {
    if let FieldKind::NestedTable(inner) = field.kind {
        let mut next_prefix: Vec<&str> = prefix.to_vec();
        next_prefix.push(field.key);
        for child in inner {
            write_field(table_name, &next_prefix, child, fields, root)?;
        }
        return Ok(());
    }

    let label = label_for(prefix, field.key);
    let Some(widget) = fields
        .iter()
        .find(|f| f.widget.label() == label)
        .map(|f| &f.widget)
    else {
        return Ok(());
    };

    let value = widget_to_toml(widget, &field.kind);
    set_path(root, table_name, prefix, field.key, value);
    Ok(())
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

fn widget_to_toml(widget: &WidgetKind, kind: &FieldKind) -> toml::Value {
    match (widget, kind) {
        (WidgetKind::Toggle(w), _) => toml::Value::Boolean(w.value),
        (WidgetKind::NumberStepper(w), FieldKind::Float { display_scale, .. }) => {
            let scale = (*display_scale).max(1) as f64;
            toml::Value::Float(w.value as f64 / scale)
        }
        (WidgetKind::NumberStepper(w), _) => toml::Value::Integer(w.value),
        (WidgetKind::TextInput(w), _) => toml::Value::String(w.value.clone()),
        (WidgetKind::Dropdown(w), _) => toml::Value::String(w.selected_value().to_string()),
        (WidgetKind::ListEditor(w), _) => {
            toml::Value::Array(w.items.iter().cloned().map(toml::Value::String).collect())
        }
        (WidgetKind::DynamicMap(w), _) => w.serialize_to_toml(),
        (WidgetKind::DynamicRows(w), _) => w.serialize_to_toml(),
    }
}

fn set_path(
    root: &mut toml::Value,
    table_name: &str,
    prefix: &[&str],
    key: &str,
    value: toml::Value,
) {
    let mut segments: Vec<&str> = table_name.split('.').collect();
    segments.extend(prefix.iter().copied());

    let mut node = root;
    for segment in &segments {
        if !node.is_table() {
            *node = toml::Value::Table(toml::map::Map::new());
        }
        let table = match node.as_table_mut() {
            Some(t) => t,
            None => return,
        };
        let entry = table
            .entry((*segment).to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        node = entry;
    }

    if !node.is_table() {
        *node = toml::Value::Table(toml::map::Map::new());
    }
    if let Some(table) = node.as_table_mut() {
        table.insert(key.to_string(), value);
    }
}

/// Run each leaf field's validator against the current widget value. Returns
/// `(field_index, feedback)` pairs for non-passing validators only.
pub(crate) fn validate_fields(
    table: &'static TableSchema,
    fields: &[SettingsField],
) -> Vec<(usize, ValidationFeedback)> {
    let mut out = Vec::new();
    let mut index = 0usize;
    for field in table.fields {
        collect_validations(&[], field, fields, &mut index, &mut out);
    }
    out
}

fn collect_validations(
    prefix: &[&str],
    field: &FieldSchema,
    fields: &[SettingsField],
    index: &mut usize,
    out: &mut Vec<(usize, ValidationFeedback)>,
) {
    if let FieldKind::NestedTable(inner) = field.kind {
        let mut next_prefix: Vec<&str> = prefix.to_vec();
        next_prefix.push(field.key);
        for child in inner {
            collect_validations(&next_prefix, child, fields, index, out);
        }
        return;
    }

    let label = label_for(prefix, field.key);
    let widget_index = *index;
    *index += 1;

    let Some(widget) = fields
        .iter()
        .find(|f| f.widget.label() == label)
        .map(|f| &f.widget)
    else {
        return;
    };

    if let Some(validator) = field.validator {
        let value = widget_to_toml(widget, &field.kind);
        if let Err(msg) = validator(&value) {
            out.push((widget_index, ValidationFeedback::error(msg)));
        }
    }
}
