//! Dynamic-section rendering tests for `docs_render` — `FieldKind::Map` and
//! `FieldKind::VecOfStruct`. Split from `docs_render_tests.rs` to keep both
//! files under the 400-line file-size cap. Static-shape regressions stay in
//! `docs_render_tests.rs`; this file only owns the new variants.

use super::*;

const AGENTS_ENTRY: &[FieldSchema] = &[
    FieldSchema {
        key: "kind",
        label: "Kind",
        help: "Agent kind (claude, codex, qwen, ...).",
        default: DefaultValue::Str(""),
        kind: FieldKind::Enum(&["claude", "codex", "qwen"]),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "enabled",
        label: "Enabled",
        help: "Whether this agent participates in sessions.",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
];

const COMMANDS_ENTRY: &[FieldSchema] = &[
    FieldSchema {
        key: "name",
        label: "Name",
        help: "Display name for the gate command.",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "command",
        label: "Command",
        help: "Shell command executed by the gate.",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
];

fn map_field(key: &'static str, help: &'static str, entry: &'static [FieldSchema]) -> FieldSchema {
    FieldSchema {
        key,
        label: key,
        help,
        default: DefaultValue::Empty,
        kind: FieldKind::Map {
            entry_fields: entry,
        },
        validator: None,
        presentation: None,
    }
}

fn vec_field(key: &'static str, help: &'static str, entry: &'static [FieldSchema]) -> FieldSchema {
    FieldSchema {
        key,
        label: key,
        help,
        default: DefaultValue::Empty,
        kind: FieldKind::VecOfStruct {
            entry_fields: entry,
        },
        validator: None,
        presentation: None,
    }
}

#[test]
fn render_type_map_returns_dynamic_map() {
    let s = render_type_string(&FieldKind::Map {
        entry_fields: AGENTS_ENTRY,
    });
    assert_eq!(s, "dynamic map");
}

#[test]
fn render_type_vec_of_struct_returns_array_of_table() {
    let s = render_type_string(&FieldKind::VecOfStruct {
        entry_fields: COMMANDS_ENTRY,
    });
    assert_eq!(s, "array of table");
}

#[test]
fn render_field_row_skips_map_field() {
    let f = map_field("entries", "Dynamic entries", AGENTS_ENTRY);
    assert!(
        render_field_row(&f).is_none(),
        "Map fields render as sections, not rows"
    );
}

#[test]
fn render_field_row_skips_vec_of_struct_field() {
    let f = vec_field("commands", "Dynamic rows", COMMANDS_ENTRY);
    assert!(
        render_field_row(&f).is_none(),
        "VecOfStruct fields render as sections, not rows"
    );
}

#[test]
fn render_table_body_emits_map_section_after_static_rows() {
    let fields: &[FieldSchema] = &[
        FieldSchema {
            key: "default",
            label: "Default",
            help: "Default agent id.",
            default: DefaultValue::Str("claude"),
            kind: FieldKind::String,
            validator: None,
            presentation: None,
        },
        map_field("entries", "Dynamic entries", AGENTS_ENTRY),
    ];
    let body = render_table_body("agents", fields);
    assert!(
        body.contains("### `[agents.<id>]` — dynamic-key map"),
        "Map heading missing, body was:\n{body}"
    );
    assert!(
        body.contains("user-chosen identifier matching `[a-z0-9_-]+`"),
        "Map prose missing regex constraint"
    );
    assert!(
        body.contains("Settings UI") && body.contains("hand-edit"),
        "Map prose missing add/remove guidance"
    );
    assert!(
        body.contains("| `default` | string | `claude` |"),
        "Static row missing"
    );
    assert!(body.contains("| `kind` | enum"), "Entry-field row missing");
    assert!(
        body.contains("| `enabled` | bool | `true` |"),
        "Entry-field row missing"
    );
}

#[test]
fn render_table_body_emits_vec_of_struct_section_with_order_note() {
    let fields: &[FieldSchema] = &[vec_field("commands", "Dynamic rows", COMMANDS_ENTRY)];
    let body = render_table_body("sessions.completion_gates.commands", fields);
    assert!(
        body.contains(
            "### `[[sessions.completion_gates.commands]]` — array-of-tables (order-sensitive)"
        ),
        "VecOfStruct heading missing, body was:\n{body}"
    );
    assert!(
        body.contains("order-sensitive — declaration order is execution order"),
        "VecOfStruct prose missing order-sensitivity note"
    );
    assert!(body.contains("| `name` | string"));
    assert!(body.contains("| `command` | string"));
}

const AGENTS_TABLE_FIELDS: &[FieldSchema] = &[FieldSchema {
    key: "entries",
    label: "entries",
    help: "Dynamic entries",
    default: DefaultValue::Empty,
    kind: FieldKind::Map {
        entry_fields: AGENTS_ENTRY,
    },
    validator: None,
    presentation: None,
}];

#[test]
fn regenerate_emits_dynamic_section_inside_markers() {
    let table = TableSchema {
        name: "agents",
        label: "Agents",
        fields: AGENTS_TABLE_FIELDS,
    };
    let doc = "# Header\n\
               <!-- BEGIN AUTOGEN:agents -->\n\
               stale body\n\
               <!-- END AUTOGEN:agents -->\n";
    let got = regenerate(doc, &[table]).expect("regenerate ok");
    assert!(got.contains("### `[agents.<id>]` — dynamic-key map"));
    assert!(got.contains("user-chosen identifier"));
    assert!(got.contains("| `kind` | enum"));
    assert!(!got.contains("stale body"));
}

#[test]
fn dynamic_section_snapshot_map() {
    let fields: &[FieldSchema] = &[map_field("entries", "Dynamic entries", AGENTS_ENTRY)];
    let body = render_table_body("agents", fields);
    insta::assert_snapshot!("dynamic_section_map", body);
}

#[test]
fn dynamic_section_snapshot_vec_of_struct() {
    let fields: &[FieldSchema] = &[vec_field("commands", "Dynamic rows", COMMANDS_ENTRY)];
    let body = render_table_body("sessions.completion_gates.commands", fields);
    insta::assert_snapshot!("dynamic_section_vec_of_struct", body);
}
