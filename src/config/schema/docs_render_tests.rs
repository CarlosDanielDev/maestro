//! Unit tests for `docs_render`. Loaded via `#[path = "docs_render_tests.rs"]`
//! so the parent module stays under the 400-line file-size cap.

use super::*;
use crate::config::schema::schema_for_config;

fn leaf(
    key: &'static str,
    default: DefaultValue,
    kind: FieldKind,
    help: &'static str,
) -> FieldSchema {
    FieldSchema {
        key,
        label: key,
        help,
        default,
        kind,
        validator: None,
        presentation: None,
    }
}

const DEMO_FIELDS_SWITCH: &[FieldSchema] = &[FieldSchema {
    key: "switch",
    label: "switch",
    help: "Toggle thing",
    default: DefaultValue::Bool(true),
    kind: FieldKind::Bool,
    validator: None,
    presentation: None,
}];

const DEMO_FIELDS_K: &[FieldSchema] = &[FieldSchema {
    key: "k",
    label: "k",
    help: "K",
    default: DefaultValue::Bool(false),
    kind: FieldKind::Bool,
    validator: None,
    presentation: None,
}];

const DEMO_FIELDS_EMPTY: &[FieldSchema] = &[];

#[test]
fn render_type_bool() {
    assert_eq!(render_type_string(&FieldKind::Bool), "bool");
}

#[test]
fn render_type_int_step_one_omits_step() {
    let s = render_type_string(&FieldKind::Int {
        min: 1,
        max: 20,
        step: 1,
    });
    assert_eq!(s, "int (1..=20)");
}

#[test]
fn render_type_int_step_nonunit_includes_step() {
    let s = render_type_string(&FieldKind::Int {
        min: 10,
        max: 100,
        step: 5,
    });
    assert_eq!(s, "int (10..=100, step 5)");
}

#[test]
fn render_type_float_with_step() {
    let s = render_type_string(&FieldKind::Float {
        min: 0.1,
        max: 100.0,
        step: 0.5,
        display_scale: 10,
    });
    assert_eq!(s, "float (0.1..=100.0, step 0.5)");
}

#[test]
fn render_type_string_kind() {
    assert_eq!(render_type_string(&FieldKind::String), "string");
}

#[test]
fn render_type_enum_multiple_variants() {
    let s = render_type_string(&FieldKind::Enum(&["merge", "squash", "rebase"]));
    assert_eq!(s, "enum (`merge`, `squash`, `rebase`)");
}

#[test]
fn render_type_enum_single_variant() {
    let s = render_type_string(&FieldKind::Enum(&["only"]));
    assert_eq!(s, "enum (`only`)");
}

#[test]
fn render_type_string_list() {
    assert_eq!(
        render_type_string(&FieldKind::StringList),
        "array of string"
    );
}

#[test]
fn render_default_bool_true() {
    assert_eq!(render_default_string(&DefaultValue::Bool(true)), "`true`");
}

#[test]
fn render_default_bool_false() {
    assert_eq!(render_default_string(&DefaultValue::Bool(false)), "`false`");
}

#[test]
fn render_default_int() {
    assert_eq!(render_default_string(&DefaultValue::Int(42)), "`42`");
}

#[test]
fn render_default_float_preserves_dot_zero() {
    assert_eq!(render_default_string(&DefaultValue::Float(5.0)), "`5.0`");
}

#[test]
fn render_default_float_fractional() {
    assert_eq!(render_default_string(&DefaultValue::Float(0.5)), "`0.5`");
}

#[test]
fn render_default_str_nonempty() {
    assert_eq!(render_default_string(&DefaultValue::Str("main")), "`main`");
}

#[test]
fn render_default_str_empty_renders_unset_without_backticks() {
    assert_eq!(render_default_string(&DefaultValue::Str("")), "unset");
}

#[test]
fn render_default_strlist_empty_renders_backtick_brackets() {
    assert_eq!(render_default_string(&DefaultValue::StrList(&[])), "`[]`");
}

#[test]
fn render_default_strlist_nonempty_renders_quoted_array() {
    assert_eq!(
        render_default_string(&DefaultValue::StrList(&["maestro:ready", "type:docs"])),
        "`[\"maestro:ready\", \"type:docs\"]`"
    );
}

#[test]
fn escape_pipes_escapes_single_pipe() {
    assert_eq!(escape_pipes("one | two"), "one \\| two");
}

#[test]
fn escape_pipes_escapes_multiple_pipes() {
    assert_eq!(escape_pipes("a|b|c"), "a\\|b\\|c");
}

#[test]
fn render_field_row_standard_bool() {
    let f = leaf(
        "enabled",
        DefaultValue::Bool(true),
        FieldKind::Bool,
        "Master switch",
    );
    let row = render_field_row(&f).expect("row emitted");
    assert_eq!(row, "| `enabled` | bool | `true` | Master switch |\n");
}

#[test]
fn render_field_row_skips_nested_table() {
    let f = leaf(
        "hollow_retry",
        DefaultValue::Nested,
        FieldKind::NestedTable(&[]),
        "Sub-table",
    );
    assert!(render_field_row(&f).is_none());
}

#[test]
fn render_field_row_escapes_pipe_in_help() {
    let f = leaf(
        "thing",
        DefaultValue::Bool(true),
        FieldKind::Bool,
        "one | two",
    );
    let row = render_field_row(&f).expect("row emitted");
    assert!(row.contains("one \\| two"), "row was: {row}");
}

#[test]
fn render_table_body_has_header_and_one_row_per_leaf() {
    let fields = &[
        leaf("a", DefaultValue::Bool(true), FieldKind::Bool, "First"),
        leaf(
            "b",
            DefaultValue::Int(2),
            FieldKind::Int {
                min: 0,
                max: 10,
                step: 1,
            },
            "Second",
        ),
        leaf(
            "n",
            DefaultValue::Nested,
            FieldKind::NestedTable(&[]),
            "Sub",
        ),
    ];
    let body = render_table_body("demo", fields);
    assert!(body.starts_with("| Field | Type | Default | Description |\n|---|---|---|---|\n"));
    let data_rows = body.lines().filter(|l| l.starts_with("| `")).count();
    assert_eq!(data_rows, 2, "two non-nested leaf rows");
}

#[test]
fn regenerate_no_markers_returns_input_unchanged() {
    let doc = "# Config\n\nSome prose.\n";
    let got = regenerate(doc, &[]).expect("regenerate ok");
    assert_eq!(got, doc);
}

#[test]
fn regenerate_replaces_known_marker_block() {
    let table = TableSchema {
        name: "demo",
        label: "Demo",
        fields: DEMO_FIELDS_SWITCH,
    };
    let doc = "# Header\n\
               <!-- BEGIN AUTOGEN:demo -->\n\
               old stale row\n\
               <!-- END AUTOGEN:demo -->\n\
               # Footer\n";
    let got = regenerate(doc, &[table]).expect("regenerate ok");
    assert!(got.starts_with("# Header\n"));
    assert!(got.ends_with("# Footer\n"));
    assert!(!got.contains("old stale row"));
    assert!(got.contains("| `switch` | bool | `true` | Toggle thing |"));
    assert!(got.contains("<!-- BEGIN AUTOGEN:demo -->"));
    assert!(got.contains("<!-- END AUTOGEN:demo -->"));
}

#[test]
fn regenerate_preserves_prose_outside_markers() {
    let table = TableSchema {
        name: "demo",
        label: "Demo",
        fields: DEMO_FIELDS_K,
    };
    let prose_before = "## prologue\n\nbefore lines\nstay intact\n\n";
    let prose_after = "\n## epilogue\n\nafter lines\nstay intact\n";
    let doc = format!(
        "{prose_before}<!-- BEGIN AUTOGEN:demo -->\nstale\n<!-- END AUTOGEN:demo -->{prose_after}"
    );
    let got = regenerate(&doc, &[table]).expect("regenerate ok");
    assert!(got.contains(prose_before));
    assert!(got.contains(prose_after));
}

#[test]
fn regenerate_unknown_section_returns_err() {
    let doc = "<!-- BEGIN AUTOGEN:nope -->\n<!-- END AUTOGEN:nope -->\n";
    let err = regenerate(doc, &[]).expect_err("unknown section must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("unknown AUTOGEN section `nope`"),
        "msg was: {msg}"
    );
}

#[test]
fn regenerate_missing_end_returns_err() {
    let table = TableSchema {
        name: "demo",
        label: "Demo",
        fields: DEMO_FIELDS_EMPTY,
    };
    let doc = "<!-- BEGIN AUTOGEN:demo -->\nbody but no end\n";
    let err = regenerate(doc, &[table]).expect_err("missing END must fail");
    let msg = format!("{err}");
    assert!(msg.contains("no matching END"), "msg was: {msg}");
}

#[test]
fn regenerate_duplicate_begin_returns_err() {
    let table = TableSchema {
        name: "demo",
        label: "Demo",
        fields: DEMO_FIELDS_EMPTY,
    };
    let doc = "<!-- BEGIN AUTOGEN:demo -->\n<!-- END AUTOGEN:demo -->\n\
               <!-- BEGIN AUTOGEN:demo -->\n<!-- END AUTOGEN:demo -->\n";
    let err = regenerate(doc, &[table]).expect_err("duplicate BEGIN must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("duplicate BEGIN AUTOGEN:demo"),
        "msg was: {msg}"
    );
}

#[test]
fn regenerate_stray_end_returns_err() {
    let doc = "<!-- END AUTOGEN:lonely -->\n";
    let err = regenerate(doc, &[]).expect_err("stray END must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("END AUTOGEN:lonely without preceding BEGIN"),
        "msg was: {msg}"
    );
}

#[test]
fn flatten_schema_includes_top_level_tables() {
    let map = flatten_schema(schema_for_config());
    assert!(map.contains_key("sessions"));
    assert!(map.contains_key("gates"));
    assert!(map.contains_key("project"));
}

#[test]
fn flatten_schema_includes_nested_dotted_tables() {
    let map = flatten_schema(schema_for_config());
    assert!(map.contains_key("sessions.hollow_retry"));
    assert!(map.contains_key("sessions.context_overflow"));
    assert!(map.contains_key("sessions.conflict"));
    assert!(map.contains_key("gates.ci_auto_fix"));
}

// Dynamic-section tests live in `docs_render_dynamic_tests.rs`.
