//! Shared synthetic-schema fixture for `schema_tab` unit and snapshot tests.
//!
//! Exposes a `TableSchema` with one field of each `FieldKind`, including a
//! `NestedTable`. `name = "synth"` has no Config home, so `from_schema`
//! falls back to defaults — keeping the renderer tests isolated from the
//! production registry.

use crate::config::Config;
use crate::config::schema::{DefaultValue, FieldKind, FieldSchema, TableSchema};

const SYNTH_BOOL_FIELD: FieldSchema = FieldSchema {
    key: "flag_a",
    label: "Flag A",
    help: "a bool",
    default: DefaultValue::Bool(true),
    kind: FieldKind::Bool,
    validator: None,
    presentation: None,
};

const SYNTH_INT_FIELD: FieldSchema = FieldSchema {
    key: "count_a",
    label: "Count A",
    help: "an int",
    default: DefaultValue::Int(7),
    kind: FieldKind::Int {
        min: 1,
        max: 20,
        step: 2,
    },
    validator: None,
    presentation: None,
};

const SYNTH_FLOAT_FIELD: FieldSchema = FieldSchema {
    key: "ratio_a",
    label: "Ratio A",
    help: "a float",
    default: DefaultValue::Float(1.7),
    kind: FieldKind::Float {
        min: 0.0,
        max: 10.0,
        step: 0.5,
        display_scale: 1,
    },
    validator: None,
    presentation: None,
};

const SYNTH_STRING_FIELD: FieldSchema = FieldSchema {
    key: "name_a",
    label: "Name A",
    help: "a string",
    default: DefaultValue::Str("hello"),
    kind: FieldKind::String,
    validator: None,
    presentation: None,
};

const SYNTH_ENUM_FIELD: FieldSchema = FieldSchema {
    key: "mode_a",
    label: "Mode A",
    help: "an enum",
    default: DefaultValue::Str("beta"),
    kind: FieldKind::Enum(&["alpha", "beta", "gamma"]),
    validator: None,
    presentation: None,
};

const SYNTH_LIST_FIELD: FieldSchema = FieldSchema {
    key: "tags_a",
    label: "Tags A",
    help: "a list",
    default: DefaultValue::StrList(&["x", "y"]),
    kind: FieldKind::StringList,
    validator: None,
    presentation: None,
};

const SYNTH_NESTED_INNER: &[FieldSchema] = &[FieldSchema {
    key: "inner_bool",
    label: "Inner Bool",
    help: "nested bool",
    default: DefaultValue::Bool(false),
    kind: FieldKind::Bool,
    validator: None,
    presentation: None,
}];

const SYNTH_NESTED_FIELD: FieldSchema = FieldSchema {
    key: "nested_a",
    label: "Nested A",
    help: "a nested table",
    default: DefaultValue::Nested,
    kind: FieldKind::NestedTable(SYNTH_NESTED_INNER),
    validator: None,
    presentation: None,
};

const SYNTH_FIELDS: &[FieldSchema] = &[
    SYNTH_BOOL_FIELD,
    SYNTH_INT_FIELD,
    SYNTH_FLOAT_FIELD,
    SYNTH_STRING_FIELD,
    SYNTH_ENUM_FIELD,
    SYNTH_LIST_FIELD,
    SYNTH_NESTED_FIELD,
];

pub(crate) const SYNTH_TABLE: TableSchema = TableSchema {
    name: "synth",
    label: "Synth",
    fields: SYNTH_FIELDS,
};

pub(crate) fn default_config() -> Config {
    let minimal = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";
    toml::from_str(minimal).expect("minimal config must parse")
}
