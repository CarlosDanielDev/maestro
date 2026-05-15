//! Drift guard between `.claude/agents/subagent-*.md` and the `[[subagents]]`
//! registry in `.maestro/templates/manifest.toml`.
//!
//! - "missing" case: a new agent file on disk without a manifest entry.
//! - "stale" case: a manifest entry without a corresponding agent file.
//! Both fail with a single diff line so the fix is mechanical.
//!
//! The pure comparator `check_drift` is unit-tested below for both error
//! shapes; the `subagent_manifest_drift` integration test pins the live
//! repo state.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_disk_slugs(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("dir entry: {e}"));
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(stem) = name.strip_suffix(".md") {
            if stem.starts_with("subagent-") {
                out.insert(stem.to_string());
            }
        }
    }
    out
}

fn collect_manifest_slugs(path: &Path) -> BTreeSet<String> {
    let bytes =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let value: toml::Value = bytes
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let arr = value
        .get("subagents")
        .and_then(|x| x.as_array())
        .unwrap_or_else(|| panic!("manifest has no [[subagents]] array"));
    arr.iter()
        .filter_map(|t| t.get("slug").and_then(|s| s.as_str()).map(String::from))
        .collect()
}

fn check_drift(disk: BTreeSet<String>, manifest: BTreeSet<String>) -> Result<(), String> {
    let missing_from_manifest: Vec<&String> = disk.difference(&manifest).collect();
    let stale_in_manifest: Vec<&String> = manifest.difference(&disk).collect();
    if missing_from_manifest.is_empty() && stale_in_manifest.is_empty() {
        return Ok(());
    }
    Err(format!(
        "subagent registry drift detected.\n\
         on disk but missing from manifest: {missing_from_manifest:?}\n\
         in manifest but no file on disk:   {stale_in_manifest:?}\n\
         Fix: edit `.maestro/templates/manifest.toml` `[[subagents]]` block."
    ))
}

#[test]
fn subagent_manifest_drift() {
    let agents_dir = manifest_dir().join(".claude/agents");
    let manifest_path = manifest_dir().join(".maestro/templates/manifest.toml");
    let disk = collect_disk_slugs(&agents_dir);
    let manifest = collect_manifest_slugs(&manifest_path);
    check_drift(disk, manifest).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn check_drift_empty_sets_returns_ok() {
    assert!(check_drift(BTreeSet::new(), BTreeSet::new()).is_ok());
}

#[test]
fn check_drift_equal_sets_returns_ok() {
    let mut a = BTreeSet::new();
    a.insert("subagent-gatekeeper".to_string());
    a.insert("subagent-architect".to_string());
    let b = a.clone();
    assert!(check_drift(a, b).is_ok());
}

#[test]
fn check_drift_disk_extra_message_names_orphan() {
    let mut disk = BTreeSet::new();
    disk.insert("subagent-gatekeeper".to_string());
    disk.insert("subagent-NEW".to_string());
    let mut manifest = BTreeSet::new();
    manifest.insert("subagent-gatekeeper".to_string());
    let err = check_drift(disk, manifest).unwrap_err();
    assert!(err.contains("subagent-NEW"), "{err}");
    assert!(err.contains("missing from manifest"), "{err}");
}

#[test]
fn check_drift_manifest_extra_message_names_orphan() {
    let mut disk = BTreeSet::new();
    disk.insert("subagent-gatekeeper".to_string());
    let mut manifest = BTreeSet::new();
    manifest.insert("subagent-gatekeeper".to_string());
    manifest.insert("subagent-ORPHAN".to_string());
    let err = check_drift(disk, manifest).unwrap_err();
    assert!(err.contains("subagent-ORPHAN"), "{err}");
    assert!(err.contains("no file on disk"), "{err}");
}
