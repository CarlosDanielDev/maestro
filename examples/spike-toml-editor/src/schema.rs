//! Schema-driven editor data layer for the spike (issue #711).
//!
//! Pure, no-TUI module. Defines `Schema`, `FieldType`, and the three core
//! operations: `load_document`, `save_document`, `edit_value`.

use std::path::Path;

use anyhow::Result;
use toml_edit::{DocumentMut, value};

#[derive(Debug, Clone)]
pub enum FieldType {
    Bool,
    Int { min: i64, max: i64 },
    String,
    Enum(&'static [&'static str]),
    Table(&'static [FieldSchema]),
}

#[derive(Debug, Clone)]
pub struct FieldSchema {
    pub name: &'static str,
    pub field_type: FieldType,
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub fields: Vec<FieldSchema>,
}

/// Edited leaf value for one of the three scalar variants the spike supports.
#[derive(Debug, Clone)]
pub enum EditedValue {
    Bool(bool),
    Int(i64),
    Str(String),
}

const PROJECT_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        name: "repo",
        field_type: FieldType::String,
    },
    FieldSchema {
        name: "base_branch",
        field_type: FieldType::String,
    },
    FieldSchema {
        name: "language",
        field_type: FieldType::String,
    },
];

const SESSIONS_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        name: "max_concurrent",
        field_type: FieldType::Int { min: 1, max: 32 },
    },
    FieldSchema {
        name: "default_model",
        field_type: FieldType::String,
    },
    FieldSchema {
        name: "default_mode",
        field_type: FieldType::Enum(&["orchestrator", "vibe", "training"]),
    },
    FieldSchema {
        name: "permission_mode",
        field_type: FieldType::Enum(&[
            "default",
            "acceptEdits",
            "bypassPermissions",
            "dontAsk",
            "plan",
            "auto",
        ]),
    },
];

const TUI_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        name: "show_mascot",
        field_type: FieldType::Bool,
    },
    FieldSchema {
        name: "theme",
        field_type: FieldType::String,
    },
];

impl Schema {
    /// Resolve the focused tab's child fields, or an empty slice if the
    /// index is out of range or the tab is not a `Table` variant.
    pub fn tab_fields(&self, idx: usize) -> &'static [FieldSchema] {
        match self.fields.get(idx).map(|f| &f.field_type) {
            Some(FieldType::Table(fields)) => fields,
            _ => &[],
        }
    }
}

pub fn maestro_schema() -> Schema {
    Schema {
        fields: vec![
            FieldSchema {
                name: "project",
                field_type: FieldType::Table(PROJECT_FIELDS),
            },
            FieldSchema {
                name: "sessions",
                field_type: FieldType::Table(SESSIONS_FIELDS),
            },
            FieldSchema {
                name: "tui",
                field_type: FieldType::Table(TUI_FIELDS),
            },
        ],
    }
}

pub fn load_document(path: &Path) -> Result<DocumentMut> {
    let raw = std::fs::read_to_string(path)?;
    let doc = raw.parse::<DocumentMut>()?;
    Ok(doc)
}

pub fn save_document(path: &Path, doc: &DocumentMut) -> Result<()> {
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

/// Mutate one leaf in place. Snapshots the existing Value's decor before
/// replacing the slot so trailing comments (e.g. `# Options: ...`) survive.
pub fn edit_value(doc: &mut DocumentMut, table: &str, key: &str, new: EditedValue) {
    let slot = &mut doc[table][key];

    let (prefix, suffix) = slot
        .as_value()
        .map(|v| (v.decor().prefix().cloned(), v.decor().suffix().cloned()))
        .unwrap_or((None, None));

    *slot = match new {
        EditedValue::Bool(b) => value(b),
        EditedValue::Int(i) => value(i),
        EditedValue::Str(s) => value(s),
    };

    if let Some(v) = slot.as_value_mut() {
        if let Some(p) = prefix.as_ref().and_then(|p| p.as_str()) {
            v.decor_mut().set_prefix(p.to_string());
        }
        if let Some(s) = suffix.as_ref().and_then(|s| s.as_str()) {
            v.decor_mut().set_suffix(s.to_string());
        }
    }
}
