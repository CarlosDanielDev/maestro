//! Transport-neutral schema describing every field in `Config`.
//!
//! The schema is the single source of truth consumed by the TUI renderer,
//! tab migrations, and the auto-generated configuration reference. The
//! types in this module are intentionally simple data records — no runtime
//! state, no closures — so the entire registry lives in `const` arrays.

#[allow(dead_code)]
mod core;
pub(crate) mod docs_render;
#[allow(dead_code)]
pub(crate) mod dynamic;
#[allow(dead_code)]
mod extras;

/// Default value for a [`FieldSchema`]. Mirrors [`FieldKind`] so the entire
/// schema can live in a `const` (no heap, no `toml::Value`, no `String`).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'static str),
    StrList(&'static [&'static str]),
    /// Marker for `Map` / `VecOfStruct` — dynamic sections start empty;
    /// per-entry defaults live on `entry_fields`.
    Empty,
    /// Marker for `NestedTable` — sub-table defaults live on inner fields.
    Nested,
}

/// Renderer hint for dynamic-cardinality fields ([`FieldKind::Map`],
/// [`FieldKind::VecOfStruct`]). `None` on a [`FieldSchema`] resolves to the
/// per-variant default via [`FieldSchema::resolved_presentation`]:
///
/// * `Map` → [`Presentation::Subtabs`]
/// * `VecOfStruct` → [`Presentation::Rows`]
///
/// Static variants ignore this hint.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    Subtabs,
    Rows,
}

/// Kind of a configuration field, used by downstream renderers/generators
/// to pick a widget and enforce a domain.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum FieldKind {
    Bool,
    Int {
        min: i64,
        max: i64,
        step: i64,
    },
    Float {
        min: f64,
        max: f64,
        step: f64,
        /// Render hint: store the value as `i64 = round(value * display_scale)`
        /// inside the stepper widget and divide on writeback. `1` means render
        /// as integer (legacy behaviour). Use powers of 10 only (`10`, `100`, …)
        /// — other values are accepted by the math but display unconventionally.
        display_scale: u32,
    },
    String,
    Enum(&'static [&'static str]),
    StringList,
    /// Reserved for future use. The registry currently models nested tables
    /// (e.g. `sessions.hollow_retry`) as top-level `TableSchema` entries with
    /// dotted `name`s, so every consumer iterates the flat slice without
    /// recursion. Kept here so downstream tickets can opt into recursive
    /// rendering without a schema-shape change.
    NestedTable(&'static [FieldSchema]),
    /// Dynamic-key sub-table. Entries share `entry_fields`; users add/remove
    /// entries at runtime. Defaults to [`Presentation::Subtabs`]. Renderer
    /// wiring lands in a follow-up — A.1 ships the type only, with no
    /// registered Map sections.
    Map {
        entry_fields: &'static [FieldSchema],
    },
    /// Like [`FieldKind::Map`] but entries live at the parent table's level
    /// directly — there is no intermediate sub-key. Used for `#[serde(flatten)]`
    /// maps such as `agents.<id>`, `modes.<id>`, `teams.<id>`, where each entry
    /// is rendered as `[<table>.<id>]` rather than `[<table>.entries.<id>]`.
    /// Defaults to [`Presentation::Subtabs`]. The renderer writes back by
    /// replacing every existing table-valued key under the parent table while
    /// preserving any scalar siblings (e.g. `agents.default`).
    FlattenedMap {
        entry_fields: &'static [FieldSchema],
    },
    /// Ordered array-of-tables. Entries share `entry_fields`; reorder is part
    /// of the edit verb set. Defaults to [`Presentation::Rows`]. Renderer
    /// wiring lands in a follow-up — A.1 ships the type only.
    VecOfStruct {
        entry_fields: &'static [FieldSchema],
    },
}

/// Pure validator. Function pointer (not closure) so [`FieldSchema`] stays
/// `Copy` and const-promotable. Returns `Err(message)` on failure.
pub(crate) type Validator = fn(&toml::Value) -> Result<(), String>;

/// Schema for one leaf field (or nested sub-table) in `Config`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct FieldSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub default: DefaultValue,
    pub kind: FieldKind,
    pub validator: Option<Validator>,
    /// Renderer hint. `None` resolves to a per-variant default via
    /// [`FieldSchema::resolved_presentation`]. Static-shape variants
    /// ignore this field.
    pub presentation: Option<Presentation>,
}

impl FieldSchema {
    /// Returns the effective presentation hint, applying the per-variant
    /// default when `self.presentation` is `None`. Returns `None` for
    /// every static variant (Bool/Int/Float/String/Enum/StringList/NestedTable).
    #[allow(dead_code)]
    pub const fn resolved_presentation(&self) -> Option<Presentation> {
        if let Some(explicit) = self.presentation {
            return Some(explicit);
        }
        match self.kind {
            FieldKind::Map { .. } | FieldKind::FlattenedMap { .. } => Some(Presentation::Subtabs),
            FieldKind::VecOfStruct { .. } => Some(Presentation::Rows),
            FieldKind::Bool
            | FieldKind::Int { .. }
            | FieldKind::Float { .. }
            | FieldKind::String
            | FieldKind::Enum(_)
            | FieldKind::StringList
            | FieldKind::NestedTable(_) => None,
        }
    }
}

/// Schema for one TOML table. Nested tables (e.g. `tui.theme`) use dotted
/// `name`s — every consumer iterates the flat slice without recursion.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct TableSchema {
    pub name: &'static str,
    pub label: &'static str,
    pub fields: &'static [FieldSchema],
}

#[allow(dead_code)]
pub(crate) fn validate_url_or_empty(value: &toml::Value) -> Result<(), String> {
    match value.as_str() {
        Some("") => Ok(()),
        Some(s) if s.starts_with("https://") && s.len() > "https://".len() => Ok(()),
        Some(s) if s.starts_with("http://") && s.len() > "http://".len() => Ok(()),
        Some(_) => Err("must be empty or start with http:// / https://".into()),
        None => Err("expected a string".into()),
    }
}

#[allow(dead_code)]
pub(crate) fn validate_non_empty(value: &toml::Value) -> Result<(), String> {
    match value.as_str() {
        Some(s) if !s.trim().is_empty() => Ok(()),
        Some(_) => Err("must not be empty".into()),
        None => Err("expected a string".into()),
    }
}

pub(crate) const PROJECT_TABLE: TableSchema = TableSchema {
    name: "project",
    label: "Project",
    fields: core::PROJECT_FIELDS,
};

pub(crate) const BUDGET_TABLE: TableSchema = TableSchema {
    name: "budget",
    label: "Budget",
    fields: core::BUDGET_FIELDS,
};

pub(crate) const AGENTS_TABLE: TableSchema = TableSchema {
    name: "agents",
    label: "Agents",
    fields: &[FieldSchema {
        key: "entries",
        label: "Entries",
        help: "Configured agent providers — one entry per `[agents.<id>]` table",
        default: DefaultValue::Empty,
        kind: FieldKind::FlattenedMap {
            entry_fields: dynamic::AGENTS_ENTRY_FIELDS,
        },
        validator: None,
        presentation: Some(Presentation::Subtabs),
    }],
};

pub(crate) const MODES_TABLE: TableSchema = TableSchema {
    name: "modes",
    label: "Modes",
    fields: &[FieldSchema {
        key: "entries",
        label: "Entries",
        help: "Configured session modes — one entry per `[modes.<id>]` table",
        default: DefaultValue::Empty,
        kind: FieldKind::FlattenedMap {
            entry_fields: dynamic::MODES_ENTRY_FIELDS,
        },
        validator: None,
        presentation: Some(Presentation::Subtabs),
    }],
};

#[allow(dead_code)]
const SCHEMA: &[TableSchema] = &[
    PROJECT_TABLE,
    TableSchema {
        name: "sessions",
        label: "Sessions",
        fields: core::SESSIONS_FIELDS,
    },
    BUDGET_TABLE,
    TableSchema {
        name: "github",
        label: "GitHub",
        fields: core::GITHUB_FIELDS,
    },
    TableSchema {
        name: "notifications",
        label: "Notifications",
        fields: core::NOTIFICATIONS_FIELDS,
    },
    TableSchema {
        name: "gates",
        label: "Gates",
        fields: extras::GATES_FIELDS,
    },
    TableSchema {
        name: "review",
        label: "Review",
        fields: extras::REVIEW_FIELDS,
    },
    AGENTS_TABLE,
    MODES_TABLE,
    TableSchema {
        name: "tui",
        label: "TUI",
        fields: extras::TUI_FIELDS,
    },
    TableSchema {
        name: "tui.theme",
        label: "Theme",
        fields: extras::TUI_THEME_FIELDS,
    },
    TableSchema {
        name: "tui.layout",
        label: "Layout",
        fields: extras::TUI_LAYOUT_FIELDS,
    },
    TableSchema {
        name: "turboquant",
        label: "TurboQuant",
        fields: extras::TURBOQUANT_FIELDS,
    },
    TableSchema {
        name: "concurrency",
        label: "Concurrency",
        fields: extras::CONCURRENCY_FIELDS,
    },
    TableSchema {
        name: "monitoring",
        label: "Monitoring",
        fields: extras::MONITORING_FIELDS,
    },
];

#[allow(dead_code)]
pub(crate) const fn schema_for_config() -> &'static [TableSchema] {
    SCHEMA
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
