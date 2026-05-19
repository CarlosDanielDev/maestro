//! Unit tests for the schema-driven settings renderer (build + sync).

use super::build::from_schema;
use super::sync::{apply_to_toml, sync_to_config, validate_fields};
use super::test_fixture::{SYNTH_TABLE, default_config};
use crate::config::schema::{DefaultValue, FieldKind, FieldSchema, TableSchema};
use crate::tui::widgets::WidgetKind;

fn empty_toml_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

// --- build::from_schema ---

#[test]
fn build_bool_field_produces_toggle_with_default_value() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::Toggle(t) = &fields[0].widget else {
        panic!("expected Toggle");
    };
    assert!(t.value);
    assert_eq!(t.label, "flag_a");
}

#[test]
fn build_int_field_produces_number_stepper_with_bounds_and_step() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::NumberStepper(s) = &fields[1].widget else {
        panic!("expected NumberStepper");
    };
    assert_eq!(s.value, 7);
    assert_eq!(s.min, 1);
    assert_eq!(s.max, 20);
    assert_eq!(s.step, 2);
    assert_eq!(s.label, "count_a");
}

#[test]
fn build_float_field_produces_number_stepper_with_rounded_value() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::NumberStepper(s) = &fields[2].widget else {
        panic!("expected NumberStepper");
    };
    assert_eq!(s.value, 2);
    assert_eq!(s.min, 0);
    assert_eq!(s.max, 10);
    assert_eq!(s.step, 1);
}

#[test]
fn build_string_field_produces_text_input_with_default_value() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::TextInput(t) = &fields[3].widget else {
        panic!("expected TextInput");
    };
    assert_eq!(t.value, "hello");
    assert_eq!(t.label, "name_a");
}

#[test]
fn build_enum_field_produces_dropdown_with_variants_and_default_index() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::Dropdown(d) = &fields[4].widget else {
        panic!("expected Dropdown");
    };
    assert_eq!(d.options, vec!["alpha", "beta", "gamma"]);
    assert_eq!(d.selected, 1);
    assert_eq!(d.label, "mode_a");
}

#[test]
fn build_enum_field_unknown_default_falls_back_to_index_zero() {
    const BAD_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "weird",
        label: "Weird",
        help: "h",
        default: DefaultValue::Str("missing"),
        kind: FieldKind::Enum(&["alpha", "beta"]),
        validator: None,
    }];
    const BAD_TABLE: TableSchema = TableSchema {
        name: "bad_synth",
        label: "Bad",
        fields: BAD_FIELDS,
    };
    let fields = from_schema(&BAD_TABLE, &default_config());
    let WidgetKind::Dropdown(d) = &fields[0].widget else {
        panic!("expected Dropdown");
    };
    assert_eq!(d.selected, 0);
}

#[test]
fn build_string_list_field_produces_list_editor_with_default_items() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::ListEditor(l) = &fields[5].widget else {
        panic!("expected ListEditor");
    };
    assert_eq!(l.items, vec!["x".to_string(), "y".to_string()]);
    assert_eq!(l.label, "tags_a");
}

#[test]
fn build_nested_table_field_expands_to_inner_fields_with_dotted_labels() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let expanded = fields
        .iter()
        .find(|f| f.widget.label() == "nested_a.inner_bool")
        .expect("nested expansion must produce dotted label");
    let WidgetKind::Toggle(t) = &expanded.widget else {
        panic!("expected Toggle for nested leaf");
    };
    assert!(!t.value);
}

#[test]
fn build_synth_table_produces_correct_total_field_count() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    assert_eq!(fields.len(), 7);
}

// --- sync::apply_to_toml ---

#[test]
fn apply_to_toml_bool_writes_boolean_value() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    if let WidgetKind::Toggle(ref mut t) = fields[0].widget {
        t.value = false;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let synth = root.get("synth").and_then(|v| v.as_table()).unwrap();
    assert_eq!(synth.get("flag_a"), Some(&toml::Value::Boolean(false)));
}

#[test]
fn apply_to_toml_int_writes_integer_value() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    if let WidgetKind::NumberStepper(ref mut s) = fields[1].widget {
        s.value = 5;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let synth = root.get("synth").and_then(|v| v.as_table()).unwrap();
    assert_eq!(synth.get("count_a"), Some(&toml::Value::Integer(5)));
}

#[test]
fn apply_to_toml_float_writes_float_value() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    if let WidgetKind::NumberStepper(ref mut s) = fields[2].widget {
        s.value = 3;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let synth = root.get("synth").and_then(|v| v.as_table()).unwrap();
    assert_eq!(synth.get("ratio_a"), Some(&toml::Value::Float(3.0)));
}

#[test]
fn apply_to_toml_string_writes_string_value() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    if let WidgetKind::TextInput(ref mut t) = fields[3].widget {
        t.value = "world".into();
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let synth = root.get("synth").and_then(|v| v.as_table()).unwrap();
    assert_eq!(
        synth.get("name_a"),
        Some(&toml::Value::String("world".into()))
    );
}

#[test]
fn apply_to_toml_enum_writes_selected_variant_as_string() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    if let WidgetKind::Dropdown(ref mut d) = fields[4].widget {
        d.selected = 2;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let synth = root.get("synth").and_then(|v| v.as_table()).unwrap();
    assert_eq!(
        synth.get("mode_a"),
        Some(&toml::Value::String("gamma".into()))
    );
}

#[test]
fn apply_to_toml_string_list_writes_array_value() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    if let WidgetKind::ListEditor(ref mut l) = fields[5].widget {
        l.items = vec!["p".into(), "q".into(), "r".into()];
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let synth = root.get("synth").and_then(|v| v.as_table()).unwrap();
    let arr = synth.get("tags_a").and_then(|v| v.as_array()).unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_str(), Some("p"));
    assert_eq!(arr[2].as_str(), Some("r"));
}

#[test]
fn apply_to_toml_nested_table_writes_inner_bool_value() {
    let mut fields = from_schema(&SYNTH_TABLE, &default_config());
    let nested_idx = fields
        .iter()
        .position(|f| f.widget.label() == "nested_a.inner_bool")
        .unwrap();
    if let WidgetKind::Toggle(ref mut t) = fields[nested_idx].widget {
        t.value = true;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SYNTH_TABLE, &fields, &mut root).unwrap();
    let nested = root
        .get("synth")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("nested_a"))
        .and_then(|v| v.as_table())
        .expect("nested_a sub-table must exist");
    assert_eq!(nested.get("inner_bool"), Some(&toml::Value::Boolean(true)));
}

// --- dotted table_name walking (Layout, Theme, Gates nested tables) ---

#[test]
fn build_from_schema_resolves_value_through_dotted_table_name() {
    use crate::config::LayoutMode;
    use crate::config::schema::schema_for_config;

    let mut config = default_config();
    config.tui.layout.mode = LayoutMode::Horizontal;

    let table = schema_for_config()
        .iter()
        .find(|t| t.name == "tui.layout")
        .expect("tui.layout schema must exist");
    let fields = from_schema(table, &config);
    let mode_field = fields
        .iter()
        .find(|f| f.widget.label() == "mode")
        .expect("layout mode field must exist");
    let WidgetKind::Dropdown(d) = &mode_field.widget else {
        panic!("expected Dropdown for layout mode");
    };
    assert_eq!(
        d.options[d.selected], "horizontal",
        "from_schema must walk dotted table_name 'tui.layout' to reach the nested toml value",
    );
}

#[test]
fn sync_to_config_writes_through_dotted_table_name_to_nested_toml_tree() {
    use crate::config::LayoutMode;
    use crate::config::schema::schema_for_config;

    let mut config = default_config();
    let table = schema_for_config()
        .iter()
        .find(|t| t.name == "tui.layout")
        .expect("tui.layout schema must exist");
    let mut fields = from_schema(table, &config);
    let mode_idx = fields
        .iter()
        .position(|f| f.widget.label() == "mode")
        .unwrap();
    if let WidgetKind::Dropdown(ref mut d) = fields[mode_idx].widget {
        let horizontal_idx = d.options.iter().position(|s| s == "horizontal").unwrap();
        d.selected = horizontal_idx;
    }
    sync_to_config(table, &fields, &mut config).expect("sync_to_config must succeed");
    assert_eq!(
        config.tui.layout.mode,
        LayoutMode::Horizontal,
        "sync_to_config must walk dotted table_name 'tui.layout' to write the nested value",
    );
}

#[test]
fn sync_to_config_dotted_table_name_does_not_create_literal_key() {
    use crate::config::schema::schema_for_config;

    let mut config = default_config();
    let table = schema_for_config()
        .iter()
        .find(|t| t.name == "tui.layout")
        .expect("tui.layout schema must exist");
    let mut fields = from_schema(table, &config);
    let mode_idx = fields
        .iter()
        .position(|f| f.widget.label() == "mode")
        .unwrap();
    if let WidgetKind::Dropdown(ref mut d) = fields[mode_idx].widget {
        let horizontal_idx = d.options.iter().position(|s| s == "horizontal").unwrap();
        d.selected = horizontal_idx;
    }
    sync_to_config(table, &fields, &mut config).expect("sync_to_config must succeed");

    let root = toml::Value::try_from(&config).expect("config must serialize back to toml");
    assert!(
        root.as_table()
            .map(|t| !t.contains_key("tui.layout"))
            .unwrap_or(true),
        "sync_to_config must not insert a literal 'tui.layout' key at the root",
    );
}

// --- sync::sync_to_config end-to-end ---

#[test]
fn sync_to_config_end_to_end_project_table_mutates_repo_only() {
    use crate::config::schema::schema_for_config;
    let project_schema = schema_for_config()
        .iter()
        .find(|t| t.name == "project")
        .expect("project schema must exist");
    let config = default_config();
    let mut fields = from_schema(project_schema, &config);
    let repo_idx = fields
        .iter()
        .position(|f| f.widget.label() == "repo")
        .unwrap();
    if let WidgetKind::TextInput(ref mut t) = fields[repo_idx].widget {
        t.value = "new_owner/new_repo".into();
    }
    let mut config_copy = config.clone();
    sync_to_config(project_schema, &fields, &mut config_copy).unwrap();
    assert_eq!(config_copy.project.repo, "new_owner/new_repo");
    assert_eq!(config_copy.project.base_branch, config.project.base_branch);
}

// --- sync::validate_fields ---

#[test]
fn validate_fields_returns_error_for_failing_validator() {
    fn reject_empty(v: &toml::Value) -> Result<(), String> {
        match v.as_str() {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err("must not be empty".into()),
        }
    }
    const VALIDATED_FIELDS: &[FieldSchema] = &[FieldSchema {
        key: "name",
        label: "Name",
        help: "a name",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: Some(reject_empty),
    }];
    const VALIDATED_TABLE: TableSchema = TableSchema {
        name: "validated_synth",
        label: "Validated Synth",
        fields: VALIDATED_FIELDS,
    };
    let fields = from_schema(&VALIDATED_TABLE, &default_config());
    let errors = validate_fields(&VALIDATED_TABLE, &fields);
    assert_eq!(errors.len(), 1);
    let (idx, fb) = &errors[0];
    assert_eq!(*idx, 0);
    assert!(fb.is_error());
    assert!(fb.message.contains("must not be empty"));
}

#[test]
fn validate_fields_returns_empty_for_all_passing_validators() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let errors = validate_fields(&SYNTH_TABLE, &fields);
    assert!(errors.is_empty());
}
