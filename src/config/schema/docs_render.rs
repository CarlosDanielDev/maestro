//! Renderer that fills in `<!-- BEGIN AUTOGEN:NAME --> ... <!-- END AUTOGEN:NAME -->`
//! blocks in `docs/configuration.md` from the config schema.
//!
//! # Ordering guarantee
//!
//! The renderer iterates `TableSchema::fields` in definition order — the same
//! order developers see in `src/config/schema/core.rs` and
//! `src/config/schema/extras.rs`. There is no HashMap-driven ordering in the
//! output: the only HashMap is the section-name lookup, which is read-only
//! during scanning and does not influence the rendered byte sequence.
//!
//! # Section coverage
//!
//! Top-level `TableSchema` entries from `schema_for_config()` are addressable
//! by their `name` (e.g. `sessions`, `gates`). Nested tables — encoded as
//! `FieldKind::NestedTable` inside a parent's fields — are addressable by the
//! dotted path `parent.field_key` (e.g. `sessions.hollow_retry`,
//! `gates.ci_auto_fix`). Unknown section names cause `regenerate` to bail.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};

use super::{DefaultValue, FieldKind, FieldSchema, TableSchema};

fn end_marker(name: &str) -> String {
    format!("<!-- END AUTOGEN:{name} -->")
}

/// Build a flat `dotted-name -> &[FieldSchema]` map covering top-level tables
/// and every nested sub-table reachable through `FieldKind::NestedTable`.
fn flatten_schema(schema: &[TableSchema]) -> HashMap<String, &'static [FieldSchema]> {
    let mut out: HashMap<String, &'static [FieldSchema]> = HashMap::new();
    for table in schema {
        collect_into(table.name, table.fields, &mut out);
    }
    out
}

fn collect_into(
    name: &str,
    fields: &'static [FieldSchema],
    out: &mut HashMap<String, &'static [FieldSchema]>,
) {
    out.insert(name.to_string(), fields);
    for field in fields {
        if let FieldKind::NestedTable(inner) = field.kind {
            let nested = format!("{name}.{}", field.key);
            collect_into(&nested, inner, out);
        }
    }
}

pub(crate) fn render_type_string(kind: &FieldKind) -> String {
    match kind {
        FieldKind::Bool => "bool".to_string(),
        FieldKind::Int { min, max, step } => {
            if *step == 1 {
                format!("int ({min}..={max})")
            } else {
                format!("int ({min}..={max}, step {step})")
            }
        }
        FieldKind::Float { min, max, step, .. } => {
            if *step == 1.0 {
                format!("float ({min:?}..={max:?})")
            } else {
                format!("float ({min:?}..={max:?}, step {step:?})")
            }
        }
        FieldKind::String => "string".to_string(),
        FieldKind::Enum(variants) => {
            let joined = variants
                .iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("enum ({joined})")
        }
        FieldKind::StringList => "array of string".to_string(),
        FieldKind::NestedTable(_) => "table".to_string(),
        FieldKind::Map { .. } => "dynamic map".to_string(),
        FieldKind::VecOfStruct { .. } => "array of table".to_string(),
    }
}

pub(crate) fn render_default_string(default: &DefaultValue) -> String {
    match default {
        DefaultValue::Bool(true) => "`true`".to_string(),
        DefaultValue::Bool(false) => "`false`".to_string(),
        DefaultValue::Int(n) => format!("`{n}`"),
        DefaultValue::Float(f) => format!("`{f:?}`"),
        DefaultValue::Str("") => "unset".to_string(),
        DefaultValue::Str(s) => format!("`{s}`"),
        DefaultValue::StrList([]) => "`[]`".to_string(),
        DefaultValue::StrList(items) => {
            let inner = items
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("`[{inner}]`")
        }
        DefaultValue::Nested => "(nested table)".to_string(),
        DefaultValue::Empty => "(empty)".to_string(),
    }
}

fn escape_pipes(s: &str) -> String {
    s.replace('|', "\\|")
}

pub(crate) fn render_field_row(field: &FieldSchema) -> Option<String> {
    if matches!(
        field.kind,
        FieldKind::NestedTable(_) | FieldKind::Map { .. } | FieldKind::VecOfStruct { .. }
    ) {
        return None;
    }
    Some(format!(
        "| `{key}` | {ty} | {default} | {desc} |\n",
        key = field.key,
        ty = render_type_string(&field.kind),
        default = render_default_string(&field.default),
        desc = escape_pipes(field.help),
    ))
}

fn render_entry_table(entry_fields: &[FieldSchema]) -> String {
    let mut out = String::new();
    out.push_str("| Field | Type | Default | Description |\n");
    out.push_str("|---|---|---|---|\n");
    for field in entry_fields {
        if let Some(row) = render_field_row(field) {
            out.push_str(&row);
        }
    }
    out
}

/// Render a dynamic-key `Map` section: `### [name.<id>] — dynamic-key map`,
/// prose explaining the dynamic shape, then a fields table for entries.
fn render_dynamic_map_section(parent_name: &str, entry_fields: &[FieldSchema]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n### `[{parent_name}.<id>]` — dynamic-key map\n\n"
    ));
    out.push_str(&format!(
        "Each `<id>` is a user-chosen identifier matching `[a-z0-9_-]+`. \
         Add or remove entries via the Settings UI or by hand-editing \
         `maestro.toml`. Each `[{parent_name}.<id>]` table has the following \
         fields:\n\n"
    ));
    out.push_str(&render_entry_table(entry_fields));
    out
}

/// Render a `VecOfStruct` section: `### [[name]] — array-of-tables
/// (order-sensitive)`, prose noting that order matters, then a fields table.
fn render_dynamic_vec_section(parent_name: &str, entry_fields: &[FieldSchema]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n### `[[{parent_name}]]` — array-of-tables (order-sensitive)\n\n"
    ));
    out.push_str(&format!(
        "Each `[[{parent_name}]]` block defines one entry. The list is \
         **order-sensitive — declaration order is execution order.** Add, \
         remove, or reorder entries via the Settings UI or by hand-editing \
         `maestro.toml`. Each entry has the following fields:\n\n"
    ));
    out.push_str(&render_entry_table(entry_fields));
    out
}

pub(crate) fn render_table_body(name: &str, fields: &[FieldSchema]) -> String {
    let static_rows: Vec<String> = fields.iter().filter_map(render_field_row).collect();
    let mut out = String::new();
    if !static_rows.is_empty() {
        out.push_str("| Field | Type | Default | Description |\n");
        out.push_str("|---|---|---|---|\n");
        for row in static_rows {
            out.push_str(&row);
        }
    }
    for field in fields {
        match field.kind {
            FieldKind::Map { entry_fields } => {
                out.push_str(&render_dynamic_map_section(name, entry_fields));
            }
            FieldKind::VecOfStruct { entry_fields } => {
                out.push_str(&render_dynamic_vec_section(name, entry_fields));
            }
            _ => {}
        }
    }
    out
}

/// Replace every `<!-- BEGIN AUTOGEN:X --> ... <!-- END AUTOGEN:X -->` block
/// in `existing` with a freshly rendered field table for table `X`. Content
/// outside markers is preserved byte-for-byte. Returns `Err` on malformed
/// marker structure or unknown section names.
pub(crate) fn regenerate(existing: &str, schema: &[TableSchema]) -> Result<String> {
    let by_name = flatten_schema(schema);
    let mut out = String::with_capacity(existing.len() + 256);
    let mut iter = existing.lines().enumerate();
    let mut seen: HashSet<String> = HashSet::new();

    while let Some((lineno, line)) = iter.next() {
        if let Some(name) = parse_begin_marker(line) {
            if !seen.insert(name.to_string()) {
                bail!(
                    "docs/configuration.md line {}: duplicate BEGIN AUTOGEN:{name}",
                    lineno + 1
                );
            }
            let fields = by_name.get(name).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "docs/configuration.md line {}: unknown AUTOGEN section `{name}` \
                     (not in schema_for_config()). Remove the markers or add the table to the schema.",
                    lineno + 1
                )
            })?;

            out.push_str(line);
            out.push('\n');
            out.push_str(&render_table_body(name, fields));

            let expected_end = end_marker(name);
            let mut found_end = false;
            for (_inner_lineno, inner) in iter.by_ref() {
                if inner.trim() == expected_end {
                    out.push_str(inner);
                    out.push('\n');
                    found_end = true;
                    break;
                }
            }
            if !found_end {
                bail!(
                    "docs/configuration.md line {}: BEGIN AUTOGEN:{name} has no matching END",
                    lineno + 1
                );
            }
        } else if let Some(stray) = parse_end_marker(line) {
            bail!(
                "docs/configuration.md line {}: END AUTOGEN:{stray} without preceding BEGIN",
                lineno + 1
            );
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if !existing.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

fn parse_begin_marker(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("<!-- BEGIN AUTOGEN:")
        .and_then(|s| s.strip_suffix(" -->"))
        .map(str::trim)
}

fn parse_end_marker(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("<!-- END AUTOGEN:")
        .and_then(|s| s.strip_suffix(" -->"))
        .map(str::trim)
}

#[cfg(test)]
#[path = "docs_render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "docs_render_dynamic_tests.rs"]
mod dynamic_tests;
