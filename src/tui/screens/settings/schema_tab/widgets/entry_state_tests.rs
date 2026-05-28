#![cfg(test)]
//! Tests for [`super::entry_state::EntryState`]. Split out to keep
//! `entry_state.rs` under the 400-LOC guardrail after the #901 nested
//! DynamicMap lift added three new test cases.

use super::entry_state::EntryState;
use super::test_fixture::TEST_AGENT_FIELDS;
use crate::tui::widgets::WidgetKind;

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
fn map_kind_now_builds_nested_dynamic_map_widget() {
    // #901 — Map kind on an entry field now produces a real
    // DynamicMapWidget (not a read-only TextInput placeholder).
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const ROLE_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "agent",
        label: "Agent",
        help: "Agent override",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "role_overrides",
        label: "Role Overrides",
        help: "Per-role overrides",
        default: DefaultValue::Empty,
        kind: FieldKind::Map {
            entry_fields: ROLE_FIELDS,
        },
        validator: None,
        presentation: None,
    }];

    let e = EntryState::build("teams", "docs", ENTRY_FIELDS, None);
    assert!(
        matches!(e.fields[0].widget, WidgetKind::DynamicMap(_)),
        "Map kind must produce a DynamicMap widget, got label {:?}",
        e.fields[0].widget.label()
    );
}

#[test]
fn nested_dynamic_map_writes_back_to_toml() {
    // #901 — round-trip from TOML → widget → TOML preserves the nested
    // sub-table contents via the widget's `serialize_to_toml`, not the
    // passthrough fallback (which is reserved for FlattenedMap /
    // VecOfStruct entry fields whose editors have not been lifted).
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const ROLE_FIELDS: &[FieldSchema] = &[
        FieldSchema {
            key: "agent",
            label: "Agent",
            help: "",
            default: DefaultValue::Str(""),
            kind: FieldKind::String,
            validator: None,
            presentation: None,
        },
        FieldSchema {
            key: "mode",
            label: "Mode",
            help: "",
            default: DefaultValue::Str(""),
            kind: FieldKind::String,
            validator: None,
            presentation: None,
        },
    ];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "role_overrides",
        label: "Role Overrides",
        help: "",
        default: DefaultValue::Empty,
        kind: FieldKind::Map {
            entry_fields: ROLE_FIELDS,
        },
        validator: None,
        presentation: None,
    }];

    let mut reviewer = toml::map::Map::new();
    reviewer.insert("agent".into(), toml::Value::String("opencode".into()));
    reviewer.insert("mode".into(), toml::Value::String("review-strict".into()));
    let mut roles = toml::map::Map::new();
    roles.insert("reviewer".into(), toml::Value::Table(reviewer));
    let mut existing = toml::map::Map::new();
    existing.insert("role_overrides".into(), toml::Value::Table(roles));
    let existing_val = toml::Value::Table(existing);

    let e = EntryState::build("teams", "docs", ENTRY_FIELDS, Some(&existing_val));
    let v = e.to_toml(ENTRY_FIELDS);
    let t = v.as_table().expect("table");
    let ro = t
        .get("role_overrides")
        .and_then(|v| v.as_table())
        .expect("role_overrides must be present");
    let reviewer = ro
        .get("reviewer")
        .and_then(|v| v.as_table())
        .expect("reviewer sub-table must survive widget writeback");
    assert_eq!(
        reviewer.get("agent").and_then(|v| v.as_str()),
        Some("opencode"),
    );
    assert_eq!(
        reviewer.get("mode").and_then(|v| v.as_str()),
        Some("review-strict"),
    );
}

#[test]
fn nested_dynamic_map_with_no_entries_omits_key_from_toml() {
    // #901 AC — empty nested DynamicMap must NOT emit `role_overrides = {}`
    // (no bare table header).
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const ROLE_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "agent",
        label: "Agent",
        help: "",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "role_overrides",
        label: "Role Overrides",
        help: "",
        default: DefaultValue::Empty,
        kind: FieldKind::Map {
            entry_fields: ROLE_FIELDS,
        },
        validator: None,
        presentation: None,
    }];

    let e = EntryState::build("teams", "alpha", ENTRY_FIELDS, None);
    let v = e.to_toml(ENTRY_FIELDS);
    let t = v.as_table().expect("table");
    assert!(
        t.get("role_overrides").is_none(),
        "empty nested DynamicMap must omit the role_overrides key entirely; got {t:?}"
    );
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

#[test]
fn flattened_map_entry_field_builds_dynamic_map_widget() {
    // #908 C2 — FlattenedMap inside an entry must now produce a live
    // DynamicMapWidget instead of the read-only TextInput placeholder.
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const INNER: &[FieldSchema] = &[FieldSchema {
        key: "value",
        label: "Value",
        help: "",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "tags",
        label: "Tags",
        help: "",
        default: DefaultValue::Empty,
        kind: FieldKind::FlattenedMap {
            entry_fields: INNER,
        },
        validator: None,
        presentation: None,
    }];

    let e = EntryState::build("section", "id", ENTRY_FIELDS, None);
    assert!(
        matches!(e.fields[0].widget, WidgetKind::DynamicMap(_)),
        "FlattenedMap inside an entry must produce a DynamicMap widget, got label {:?}",
        e.fields[0].widget.label()
    );
}

#[test]
fn vec_of_struct_entry_field_builds_dynamic_rows_widget() {
    // #908 C2 — VecOfStruct inside an entry must now produce a live
    // DynamicRowsWidget instead of the read-only TextInput placeholder.
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const INNER: &[FieldSchema] = &[FieldSchema {
        key: "name",
        label: "Name",
        help: "",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "rows",
        label: "Rows",
        help: "",
        default: DefaultValue::Empty,
        kind: FieldKind::VecOfStruct {
            entry_fields: INNER,
        },
        validator: None,
        presentation: None,
    }];

    let e = EntryState::build("section", "id", ENTRY_FIELDS, None);
    assert!(
        matches!(e.fields[0].widget, WidgetKind::DynamicRows(_)),
        "VecOfStruct inside an entry must produce a DynamicRows widget, got label {:?}",
        e.fields[0].widget.label()
    );
}

#[test]
fn empty_flattened_map_omitted_from_toml() {
    // #908 C2 — empty FlattenedMap widget must not emit a bare table header.
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const INNER: &[FieldSchema] = &[FieldSchema {
        key: "value",
        label: "Value",
        help: "",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "tags",
        label: "Tags",
        help: "",
        default: DefaultValue::Empty,
        kind: FieldKind::FlattenedMap {
            entry_fields: INNER,
        },
        validator: None,
        presentation: None,
    }];

    let e = EntryState::build("section", "id", ENTRY_FIELDS, None);
    let v = e.to_toml(ENTRY_FIELDS);
    let t = v.as_table().expect("table");
    assert!(
        t.get("tags").is_none(),
        "empty FlattenedMap must omit the key entirely; got {t:?}"
    );
}

#[test]
fn empty_vec_of_struct_omitted_from_toml() {
    // #908 C2 — empty VecOfStruct widget must not emit a bare array.
    use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
    const INNER: &[FieldSchema] = &[FieldSchema {
        key: "name",
        label: "Name",
        help: "",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }];
    const ENTRY_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "rows",
        label: "Rows",
        help: "",
        default: DefaultValue::Empty,
        kind: FieldKind::VecOfStruct {
            entry_fields: INNER,
        },
        validator: None,
        presentation: None,
    }];

    let e = EntryState::build("section", "id", ENTRY_FIELDS, None);
    let v = e.to_toml(ENTRY_FIELDS);
    let t = v.as_table().expect("table");
    assert!(
        t.get("rows").is_none(),
        "empty VecOfStruct must omit the key entirely; got {t:?}"
    );
}
