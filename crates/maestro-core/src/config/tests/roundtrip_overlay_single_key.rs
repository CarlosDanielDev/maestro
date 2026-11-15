//! Single-field-change byte-identity tests for `Config::save_into_str`
//! (issue #712). Each test asserts that mutating one field on `Config` only
//! changes the corresponding line in the output; every other line stays
//! byte-identical to the original.

use super::roundtrip_overlay::{
    assert_lines_identical_except, assert_single_field_save, fixture, temp_file_with,
};
use super::*;

#[test]
fn single_key_change_bool_gates_enabled() {
    // Section-scoped outlier: `enabled = true` appears under multiple
    // sections in the fixture, so a global trim-match isn't unique. Scan
    // backward from each candidate to find the enclosing section header.
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");

    cfg.gates.enabled = false;

    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");

    let lines: Vec<&str> = original_text.lines().collect();
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if l.trim() == "enabled = true"
                && lines[..i]
                    .iter()
                    .rev()
                    .find(|prev| prev.starts_with('['))
                    .map(|h| h.trim() == "[gates]")
                    .unwrap_or(false)
            {
                Some(i + 1)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "expected one [gates].enabled line; found {changed:?}"
    );
    assert_lines_identical_except(&original_text, &result, &changed);
    assert!(result.contains("enabled = false"));
}

#[test]
fn single_key_change_int_budget_alert_threshold() {
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");
    cfg.budget.alert_threshold_pct = 95;
    assert_single_field_save(
        &cfg,
        &original_text,
        "alert_threshold_pct = 80",
        "alert_threshold_pct = 95",
    );
}

#[test]
fn single_key_change_string_project_base_branch() {
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");
    cfg.project.base_branch = "develop".to_string();
    assert_single_field_save(
        &cfg,
        &original_text,
        r#"base_branch = "main""#,
        r#"base_branch = "develop""#,
    );
}

#[test]
fn single_key_change_enum_layout_mode() {
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");
    cfg.tui.layout.mode = LayoutMode::Horizontal;
    assert_single_field_save(
        &cfg,
        &original_text,
        r#"mode = "vertical""#,
        r#"mode = "horizontal""#,
    );
}

#[test]
fn single_key_change_list_sessions_allowed_tools() {
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");
    cfg.sessions.allowed_tools = vec!["Read".to_string(), "Write".to_string()];

    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");

    let changed: Vec<usize> = original_text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| (l.trim() == "allowed_tools = []").then_some(i + 1))
        .collect();
    assert_eq!(changed.len(), 1);
    assert_lines_identical_except(&original_text, &result, &changed);
    assert!(
        result.contains(r#"allowed_tools = ["Read", "Write"]"#)
            || result.contains(r#"allowed_tools = ["Read","Write"]"#),
        "result must contain updated allowed_tools: {result}"
    );
}

#[test]
fn single_key_change_nested_table_field_overflow_threshold() {
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");
    cfg.sessions.context_overflow.overflow_threshold_pct = 85;
    assert_single_field_save(
        &cfg,
        &original_text,
        "overflow_threshold_pct = 70",
        "overflow_threshold_pct = 85",
    );
}
