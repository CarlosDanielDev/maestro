//! Integration tests for PrMarker round-trip behaviour (issue #735).
//!
//! External integration test reaching the `pub` surface of
//! `maestro::work::pr_marker`. Covers backward-compat (legacy markers
//! without `issue_number`), full round-trip, atomic write, and the two
//! typed error variants.

use chrono::{DateTime, Utc};
use maestro::work::pr_marker::{MarkerError, PrMarker};
use std::path::PathBuf;

fn fixed_ts() -> DateTime<Utc> {
    "2024-01-15T10:30:00Z"
        .parse::<DateTime<Utc>>()
        .expect("static timestamp must parse")
}

fn marker_with_issue() -> PrMarker {
    PrMarker {
        pr_number: 99,
        owner: "acme".to_string(),
        repo: "api".to_string(),
        issue_number: Some(735),
        ts: fixed_ts(),
    }
}

/// `.tmp` staging path matches the shell writer: append `.tmp` to the full
/// name (NOT replace the extension).
fn tmp_sibling(path: &std::path::Path) -> PathBuf {
    let mut s = path.to_path_buf().into_os_string();
    s.push(".tmp");
    s.into()
}

#[test]
fn old_marker_reads_without_issue_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("marker.json");

    // JSON written by the old /pushup template (no issue_number key).
    let legacy_json = r#"{"pr_number":42,"owner":"acme","repo":"api","ts":"2024-01-15T10:30:00Z"}"#;
    std::fs::write(&path, legacy_json).expect("write legacy marker");

    let result = PrMarker::read(&path).expect("legacy marker must parse");

    assert_eq!(result.pr_number, 42);
    assert_eq!(result.owner, "acme");
    assert_eq!(result.repo, "api");
    assert_eq!(
        result.issue_number, None,
        "missing issue_number must deserialize as None"
    );
}

#[test]
fn new_marker_roundtrips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("marker.json");
    let original = marker_with_issue();

    original
        .write_atomic(&path)
        .expect("write_atomic must not fail");

    let roundtripped = PrMarker::read(&path).expect("read must not fail");
    assert_eq!(
        roundtripped, original,
        "round-tripped marker must equal original"
    );
}

#[test]
fn new_marker_json_has_issue_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("marker.json");
    let m = marker_with_issue();

    m.write_atomic(&path).expect("write_atomic must not fail");

    let raw = std::fs::read_to_string(&path).expect("read file");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON on disk");

    assert_eq!(
        json["issue_number"], 735,
        "issue_number key must be present in the written JSON"
    );
    assert_eq!(json["pr_number"], 99);
}

#[test]
fn write_is_atomic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("marker.json");
    let tmp_path = tmp_sibling(&path);
    let m = marker_with_issue();

    m.write_atomic(&path).expect("write_atomic must not fail");

    assert!(
        !tmp_path.exists(),
        ".tmp staging file must be removed after write_atomic completes"
    );

    let result = PrMarker::read(&path).expect("marker must be readable after write_atomic");
    assert_eq!(
        result, m,
        "marker must parse back to original after atomic write"
    );
}

#[test]
fn malformed_json_returns_parse_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("marker.json");
    std::fs::write(&path, b"not json at all").expect("write malformed content");

    let err = PrMarker::read(&path).expect_err("malformed JSON must produce an error");

    assert!(
        matches!(err, MarkerError::Parse(_)),
        "expected MarkerError::Parse, got: {err:?}"
    );
}

#[test]
fn missing_file_returns_read_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("does_not_exist.json");
    assert!(!path.exists(), "pre-condition: path must not exist");

    let err = PrMarker::read(&path).expect_err("missing file must produce an error");

    match err {
        MarkerError::Read { path: ref p, .. } => {
            assert_eq!(
                *p,
                path.to_string_lossy().to_string(),
                "MarkerError::Read must carry the stringified path"
            );
        }
        other => panic!("expected MarkerError::Read, got: {other:?}"),
    }
}
