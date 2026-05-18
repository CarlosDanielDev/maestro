//! Byte-identity round-trip tests for `Config::save_into_str` (issue #712).
//!
//! These exercise the `toml_edit::DocumentMut` overlay path: comments, blank
//! lines, key order, and unknown sections must survive a save unchanged.

use super::*;
use std::io::Write;

pub(super) fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/config_roundtrip")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}

pub(super) fn assert_byte_identical(label_a: &str, a: &str, label_b: &str, b: &str) {
    if a == b {
        return;
    }
    let lines_a: Vec<&str> = a.lines().collect();
    let lines_b: Vec<&str> = b.lines().collect();
    let max = lines_a.len().max(lines_b.len());
    let mut diff = String::new();
    for i in 0..max {
        let la = lines_a.get(i).copied().unwrap_or("<missing>");
        let lb = lines_b.get(i).copied().unwrap_or("<missing>");
        if la != lb {
            diff.push_str(&format!(
                "line {}: {label_a}={la:?}  {label_b}={lb:?}\n",
                i + 1
            ));
        }
    }
    panic!(
        "strings are not byte-identical.\nFirst differences:\n{diff}\n\
         {label_a} len={} lines   {label_b} len={} lines",
        lines_a.len(),
        lines_b.len()
    );
}

pub(super) fn assert_lines_identical_except(
    before: &str,
    after: &str,
    changed_line_indices: &[usize],
) {
    let lines_before: Vec<&str> = before.lines().collect();
    let lines_after: Vec<&str> = after.lines().collect();
    assert_eq!(
        lines_before.len(),
        lines_after.len(),
        "line count changed: before={} after={}\n--- before ---\n{}\n--- after ---\n{}",
        lines_before.len(),
        lines_after.len(),
        before,
        after,
    );
    for (i, (lb, la)) in lines_before.iter().zip(lines_after.iter()).enumerate() {
        let one_based = i + 1;
        if changed_line_indices.contains(&one_based) {
            continue;
        }
        assert_eq!(
            lb, la,
            "unexpected diff at line {one_based}: before={lb:?} after={la:?}"
        );
    }
}

pub(super) fn temp_file_with(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(content.as_bytes()).expect("write tempfile");
    f.flush().expect("flush");
    f
}

/// Asserts that `cfg.save_into_str(original_text)` changes exactly the line
/// whose trimmed text matches `expected_old_line`, and that the result
/// contains `expected_new_substr`. Use for single-field-change round-trip
/// tests where the original fixture contains that line exactly once.
pub(super) fn assert_single_field_save(
    cfg: &Config,
    original_text: &str,
    expected_old_line: &str,
    expected_new_substr: &str,
) {
    let result = cfg
        .save_into_str(original_text)
        .expect("save_into_str must succeed");
    let changed: Vec<usize> = original_text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| (l.trim() == expected_old_line).then_some(i + 1))
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "expected exactly one line matching {expected_old_line:?}; got {changed:?}"
    );
    assert_lines_identical_except(original_text, &result, &changed);
    assert!(
        result.contains(expected_new_substr),
        "result must contain {expected_new_substr:?}:\n{result}"
    );
}

#[test]
fn byte_identical_round_trip_full_maestro() {
    let original_text = fixture("full_maestro.toml");
    let tmp = temp_file_with(&original_text);
    let cfg = Config::load(tmp.path()).expect("full_maestro.toml must parse");
    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");
    assert_byte_identical("original", &original_text, "after_save", &result);
}

#[test]
fn byte_identical_round_trip_with_comments() {
    let original_text = fixture("with_comments.toml");
    let tmp = temp_file_with(&original_text);
    let cfg = Config::load(tmp.path()).expect("with_comments.toml must parse");
    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");
    assert_byte_identical("original", &original_text, "after_save", &result);
}

#[test]
fn byte_identical_round_trip_with_unknown_section() {
    let original_text = fixture("with_unknown_section.toml");
    let tmp = temp_file_with(&original_text);
    let cfg = Config::load(tmp.path()).expect("with_unknown_section.toml must parse");
    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");
    assert_byte_identical("original", &original_text, "after_save", &result);
}

#[test]
fn save_returns_err_on_malformed_input_file_untouched() {
    let malformed = "[project\nrepo = \"owner/repo\"\n";
    let tmp = temp_file_with(malformed);

    let valid_toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";
    let good_tmp = temp_file_with(valid_toml);
    let cfg = Config::load(good_tmp.path()).expect("minimal config must load");

    let result = cfg.save_into_existing(tmp.path());
    assert!(
        result.is_err(),
        "save_into_existing must return Err on malformed file"
    );

    let on_disk = std::fs::read_to_string(tmp.path()).expect("temp file must still be readable");
    assert_eq!(
        on_disk, malformed,
        "malformed file must not have been modified"
    );
}

#[test]
fn save_creates_file_when_path_does_not_exist() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("maestro.toml");
    assert!(!target.exists(), "target must not exist before save");

    let valid_toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";
    let src_tmp = temp_file_with(valid_toml);
    let cfg = Config::load(src_tmp.path()).expect("config load");

    cfg.save(&target)
        .expect("save must succeed on non-existent path");

    assert!(target.exists(), "save must create the file");
    let reloaded = Config::load(&target).expect("saved file must be parseable");
    assert_eq!(cfg, reloaded, "round-trip must preserve config values");
}

#[test]
fn comment_adjacent_to_changed_key_preserved() {
    let original_text = fixture("with_comments.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");

    cfg.sessions.stall_timeout_secs = 500;

    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");

    assert!(
        result.contains("stall_timeout_secs = 500"),
        "result must contain updated stall_timeout_secs:\n{result}"
    );
    assert!(
        result.contains("# seconds; increase for slow LLMs"),
        "inline comment on changed line must survive:\n{result}"
    );
    assert!(
        result.contains("# Session limits and defaults"),
        "section header comment must survive"
    );
    assert_eq!(
        original_text.lines().count(),
        result.lines().count(),
        "line count must not change"
    );
}

#[test]
fn unknown_section_survives_save() {
    let original_text = fixture("with_unknown_section.toml");
    let tmp = temp_file_with(&original_text);
    let cfg = Config::load(tmp.path()).expect("load");

    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");

    assert_byte_identical("original", &original_text, "after_save", &result);

    assert!(result.contains("[custom_automation]"));
    assert!(result.contains(r#"foo = "bar""#));
    assert!(result.contains("baz = 42"));
    assert!(result.contains("[custom_automation.nested]"));
    assert!(result.contains("deep_key = true"));
}

#[test]
fn unknown_section_survives_change_to_known_section() {
    let original_text = fixture("with_unknown_section.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");

    cfg.budget.per_session_usd = 9.99;

    let result = cfg
        .save_into_str(&original_text)
        .expect("save_into_str must succeed");

    assert!(result.contains("per_session_usd = 9.99"));
    assert!(result.contains("[custom_automation]"));
    assert!(result.contains(r#"foo = "bar""#));
    assert!(result.contains("baz = 42"));
    assert!(result.contains("[custom_automation.nested]"));
    assert!(result.contains("deep_key = true"));
    assert_eq!(
        original_text.lines().count(),
        result.lines().count(),
        "line count must not change"
    );
}

#[test]
fn save_into_existing_writes_atomically_and_preserves_comments() {
    let original_text = fixture("with_comments.toml");
    let tmp = temp_file_with(&original_text);
    let mut cfg = Config::load(tmp.path()).expect("load");

    cfg.budget.alert_threshold_pct = 90;

    cfg.save_into_existing(tmp.path())
        .expect("save_into_existing must succeed");

    let on_disk = std::fs::read_to_string(tmp.path()).expect("read back");

    assert!(on_disk.contains("# Financial guardrails"));
    assert!(on_disk.contains("alert_threshold_pct = 90"));
    assert!(
        on_disk.contains("# notify at 80 %") || on_disk.contains("# notify at"),
        "inline comment must survive: {on_disk}"
    );
}
