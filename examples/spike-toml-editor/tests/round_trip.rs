//! Round-trip tests for the schema-driven TOML editor spike (issue #711).
//!
//! These tests cover the pure data layer only — no TUI, no terminal.
//! Run from inside examples/spike-toml-editor/:
//!     cargo test

use std::path::PathBuf;

use spike_toml_editor::{
    EditedValue, FieldType, edit_value, load_document, maestro_schema, save_document,
};

fn fixture_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("fixtures")
        .join("maestro.toml")
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(fixture_path()).expect("fixture must exist")
}

// ---------------------------------------------------------------------------
// 1. Schema shape
// ---------------------------------------------------------------------------

#[test]
fn schema_has_three_tabs() {
    let schema = maestro_schema();
    assert_eq!(
        schema.fields.len(),
        3,
        "maestro_schema must return exactly 3 top-level tabs"
    );
    let names: Vec<&str> = schema.fields.iter().map(|f| f.name).collect();
    assert_eq!(names, vec!["project", "sessions", "tui"]);
}

#[test]
fn schema_sessions_tab_has_expected_fields() {
    let schema = maestro_schema();
    let sessions = schema
        .fields
        .iter()
        .find(|f| f.name == "sessions")
        .expect("sessions tab must exist");

    let inner = match &sessions.field_type {
        FieldType::Table(fields) => fields,
        other => panic!("sessions must be Table, got {:?}", other),
    };

    let field_names: Vec<&str> = inner.iter().map(|f| f.name).collect();
    assert!(
        field_names.contains(&"max_concurrent"),
        "sessions must have max_concurrent"
    );
    assert!(
        field_names.contains(&"permission_mode"),
        "sessions must have permission_mode"
    );

    let max_concurrent = inner.iter().find(|f| f.name == "max_concurrent").unwrap();
    assert!(
        matches!(max_concurrent.field_type, FieldType::Int { .. }),
        "max_concurrent must be FieldType::Int"
    );

    let perm = inner.iter().find(|f| f.name == "permission_mode").unwrap();
    let variants = match &perm.field_type {
        FieldType::Enum(v) => v,
        other => panic!("permission_mode must be Enum, got {:?}", other),
    };
    assert!(
        variants.contains(&"bypassPermissions"),
        "permission_mode Enum must include bypassPermissions"
    );
    assert!(
        variants.contains(&"plan"),
        "permission_mode Enum must include plan"
    );
}

// ---------------------------------------------------------------------------
// 2. load_document
// ---------------------------------------------------------------------------

#[test]
fn load_document_parses_fixture() {
    let doc = load_document(&fixture_path()).expect("fixture must parse");
    assert_eq!(
        doc["project"]["repo"].as_str(),
        Some("CarlosDanielDev/maestro"),
        "project.repo must match fixture value"
    );
}

#[test]
fn load_document_errors_on_missing_file() {
    let missing = PathBuf::from("/tmp/this-file-does-not-exist-711.toml");
    assert!(
        load_document(&missing).is_err(),
        "load_document must return Err for a missing file"
    );
}

// ---------------------------------------------------------------------------
// 3. save_document
// ---------------------------------------------------------------------------

#[test]
fn save_document_writes_parseable_toml() {
    let doc = load_document(&fixture_path()).expect("fixture must parse");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");

    save_document(tmp.path(), &doc).expect("save_document must succeed");

    let written = std::fs::read_to_string(tmp.path()).expect("read back");
    let reparsed: Result<toml_edit::DocumentMut, _> = written.parse();
    assert!(
        reparsed.is_ok(),
        "saved file must be valid TOML, parse error: {:?}",
        reparsed.err()
    );
}

// ---------------------------------------------------------------------------
// 4. edit_value — field mutations
// ---------------------------------------------------------------------------

#[test]
fn edit_bool_value_changes_field() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(&mut doc, "tui", "show_mascot", EditedValue::Bool(false));

    assert_eq!(
        doc["tui"]["show_mascot"].as_bool(),
        Some(false),
        "show_mascot must be false after edit"
    );
}

#[test]
fn edit_int_value_changes_field() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(&mut doc, "sessions", "max_concurrent", EditedValue::Int(8));

    assert_eq!(
        doc["sessions"]["max_concurrent"].as_integer(),
        Some(8),
        "max_concurrent must be 8 after edit"
    );
}

#[test]
fn edit_enum_value_changes_field() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(
        &mut doc,
        "sessions",
        "permission_mode",
        EditedValue::Str("plan".to_string()),
    );

    assert_eq!(
        doc["sessions"]["permission_mode"].as_str(),
        Some("plan"),
        "permission_mode must be 'plan' after edit"
    );
}

// ---------------------------------------------------------------------------
// 5. Decor-preservation canaries (the spike's empirical question)
// ---------------------------------------------------------------------------

#[test]
fn trailing_comment_preserved_after_enum_edit() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(
        &mut doc,
        "sessions",
        "permission_mode",
        EditedValue::Str("plan".to_string()),
    );

    let serialized = doc.to_string();
    let perm_line = serialized
        .lines()
        .find(|l| l.contains("permission_mode"))
        .expect("permission_mode line must exist in serialized output");

    assert!(
        perm_line.contains("# Options:"),
        "trailing '# Options:' comment must survive enum edit. Got: {:?}",
        perm_line
    );
}

// ---------------------------------------------------------------------------
// 6. Layout preservation canaries
// ---------------------------------------------------------------------------

#[test]
fn leading_comment_block_preserved() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(&mut doc, "tui", "show_mascot", EditedValue::Bool(false));

    let serialized = doc.to_string();
    assert!(
        serialized.starts_with("# maestro spike fixture"),
        "file-level leading comment must be preserved after edit; got start: {:?}",
        &serialized[..serialized.len().min(60)]
    );
}

#[test]
fn blank_lines_between_sections_preserved() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(&mut doc, "sessions", "max_concurrent", EditedValue::Int(8));

    let serialized = doc.to_string();
    let double_newline_count = serialized.match_indices("\n\n").count();
    assert!(
        double_newline_count >= 2,
        "at least 2 blank-line section separators must survive round-trip, found {}",
        double_newline_count
    );
}

#[test]
fn plugins_empty_array_and_comment_preserved() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");

    edit_value(&mut doc, "sessions", "max_concurrent", EditedValue::Int(8));

    let serialized = doc.to_string();
    assert!(
        serialized.contains("plugins = []"),
        "plugins empty array literal must be preserved"
    );
    assert!(
        serialized.contains("# Empty = no plugins loaded"),
        "plugins trailing comment must be preserved"
    );
}

// ---------------------------------------------------------------------------
// 7. End-to-end demo flow
// ---------------------------------------------------------------------------

#[test]
fn round_trip_demo_flow() {
    let mut doc = load_document(&fixture_path()).expect("fixture must parse");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");

    edit_value(&mut doc, "tui", "show_mascot", EditedValue::Bool(false));
    edit_value(
        &mut doc,
        "sessions",
        "permission_mode",
        EditedValue::Str("plan".to_string()),
    );
    edit_value(&mut doc, "sessions", "max_concurrent", EditedValue::Int(8));

    save_document(tmp.path(), &doc).expect("save_document must succeed");

    let reloaded = load_document(tmp.path()).expect("reloaded doc must parse");

    assert_eq!(reloaded["tui"]["show_mascot"].as_bool(), Some(false));
    assert_eq!(
        reloaded["sessions"]["permission_mode"].as_str(),
        Some("plan")
    );
    assert_eq!(reloaded["sessions"]["max_concurrent"].as_integer(), Some(8));
}

#[test]
fn round_trip_unmodified_keys_byte_identical() {
    let original = String::from_utf8(fixture_bytes()).expect("fixture is valid UTF-8");

    let mut doc = load_document(&fixture_path()).expect("fixture must parse");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");

    edit_value(&mut doc, "tui", "show_mascot", EditedValue::Bool(false));
    edit_value(
        &mut doc,
        "sessions",
        "permission_mode",
        EditedValue::Str("plan".to_string()),
    );
    edit_value(&mut doc, "sessions", "max_concurrent", EditedValue::Int(8));

    save_document(tmp.path(), &doc).expect("save must succeed");
    let saved = std::fs::read_to_string(tmp.path()).expect("read saved");

    let unmodified_originals: Vec<&str> = original
        .lines()
        .filter(|l| {
            !l.contains("show_mascot")
                && !l.contains("permission_mode")
                && !l.contains("max_concurrent")
        })
        .collect();

    let saved_lines: Vec<&str> = saved.lines().collect();
    for orig_line in &unmodified_originals {
        assert!(
            saved_lines.contains(orig_line),
            "unmodified line missing or corrupted in saved output.\nExpected: {:?}",
            orig_line
        );
    }
}
