//! Issue #839 — golden-rules entry-file sync end-to-end test.
//!
//! Builds a tempdir replica of the maestro repo skeleton (manifest +
//! canonical golden-rules + two entry-point files), exercises the splice
//! logic via `golden_rules::splice`, and asserts marker-bounded content
//! lands intact while content outside the markers is preserved.

use std::fs;

use maestro::commands::sync_templates::golden_rules::{GoldenRulesError, splice};

#[test]
fn splice_round_trip_against_real_repo_canonical_body_is_idempotent() {
    let repo = std::env::current_dir().expect("cwd");
    let canonical_path = repo.join(".maestro/templates/core/golden-rules.md");
    let canonical = fs::read_to_string(&canonical_path).expect("read canonical");

    for entry_rel in [
        ".claude/CLAUDE.md",
        ".codex/AGENTS.md",
        "AGENTS.md",
        "GEMINI.md",
    ] {
        let entry_path = repo.join(entry_rel);
        let current =
            fs::read_to_string(&entry_path).unwrap_or_else(|e| panic!("read {entry_rel}: {e}"));
        let spliced = splice(&current, &canonical, &entry_path)
            .unwrap_or_else(|e| panic!("splice {entry_rel}: {e}"));
        assert_eq!(
            current, spliced,
            "entry file `{entry_rel}` already matches canonical; splice must be a no-op"
        );
    }
}

#[test]
fn splice_drift_detected_on_mutated_entry_file() {
    let repo = std::env::current_dir().expect("cwd");
    let canonical = fs::read_to_string(repo.join(".maestro/templates/core/golden-rules.md"))
        .expect("canonical");
    let entry = fs::read_to_string(repo.join(".claude/CLAUDE.md")).expect("entry");

    // Mutate the block content by replacing a known sentence.
    let mutated = entry.replace("## Who I am", "## Who I am — TAMPERED HEADING");
    assert_ne!(entry, mutated, "test setup must mutate the file");

    let respliced = splice(
        &mutated,
        &canonical,
        std::path::Path::new("/test/claude.md"),
    )
    .expect("splice ok");
    assert_eq!(
        respliced, entry,
        "splice must restore canonical content even when entry block was tampered with"
    );
}

#[test]
fn splice_error_when_begin_marker_missing() {
    let path = std::path::Path::new("/test/entry.md");
    let err = splice(
        "no markers here\n<!-- END GOLDEN-RULES -->\n",
        "body\n",
        path,
    )
    .unwrap_err();
    assert!(matches!(err, GoldenRulesError::BeginMarkerMissing { .. }));
}
