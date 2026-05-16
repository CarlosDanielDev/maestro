//! Tests for `scaffold.rs`. Sibling file loaded via `#[path]` to keep
//! `scaffold.rs` under the repo's 400-line guardrail.

use super::{FsScaffolder, ScaffoldAction, Scaffolder, scaffold_templates_dir};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

struct InMemoryScaffolder {
    files: RefCell<HashMap<PathBuf, Vec<u8>>>,
}

impl InMemoryScaffolder {
    fn new() -> Self {
        Self {
            files: RefCell::new(HashMap::new()),
        }
    }

    fn with_file(path: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> Self {
        let s = Self::new();
        s.files.borrow_mut().insert(path.into(), contents.into());
        s
    }

    fn get(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.borrow().get(path).cloned()
    }
}

impl Scaffolder for InMemoryScaffolder {
    fn write(&self, rel_path: &Path, contents: &[u8]) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(rel_path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn exists(&self, rel_path: &Path) -> bool {
        self.files.borrow().contains_key(rel_path)
    }
}

fn expected_paths() -> Vec<PathBuf> {
    [
        ".maestro/templates/README.md",
        ".maestro/templates/manifest.toml",
        ".maestro/templates/core/premises.md",
        ".maestro/templates/core/tdd-cycle.md",
        ".maestro/templates/core/dependency-graph.md",
        ".maestro/templates/commands/.gitkeep",
        ".maestro/templates/commands/implement.md",
        ".maestro/templates/commands/plan-feature.md",
        ".maestro/templates/commands/pushup.md",
        ".maestro/templates/commands/simplify.md",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[test]
fn scaffold_writes_all_10_files_into_empty_scaffolder() {
    let s = InMemoryScaffolder::new();
    let report = scaffold_templates_dir(&s).expect("scaffold must not error");

    let mut written: Vec<PathBuf> = report.files.iter().map(|f| f.path.clone()).collect();
    written.sort();

    let mut expected = expected_paths();
    expected.sort();

    assert_eq!(
        written, expected,
        "scaffolder must write exactly the 10 template files"
    );
}

#[test]
fn scaffold_report_has_exactly_10_entries_all_created() {
    let s = InMemoryScaffolder::new();
    let report = scaffold_templates_dir(&s).expect("scaffold must not error");

    assert_eq!(
        report.files.len(),
        10,
        "expected exactly 10 ScaffoldedFile entries"
    );
    assert_eq!(
        report.count(ScaffoldAction::Created),
        10,
        "all 10 files must be Created on a fresh run"
    );
    assert_eq!(
        report.count(ScaffoldAction::Skipped),
        0,
        "no files must be Skipped on a fresh run"
    );
}

#[test]
fn scaffold_idempotent_skips_pre_existing_file_and_preserves_content() {
    let original_content = b"# user-edited content that must survive";
    let manifest_path = PathBuf::from(".maestro/templates/manifest.toml");

    let s = InMemoryScaffolder::with_file(manifest_path.clone(), original_content.to_vec());
    let report = scaffold_templates_dir(&s).expect("scaffold must not error");

    assert_eq!(
        report.files.len(),
        10,
        "report must still contain 10 entries"
    );
    assert_eq!(
        report.count(ScaffoldAction::Skipped),
        1,
        "exactly 1 file was pre-existing — must be Skipped"
    );
    assert_eq!(
        report.count(ScaffoldAction::Created),
        9,
        "remaining 9 files must be Created"
    );

    let manifest_entry = report
        .files
        .iter()
        .find(|f| f.path == manifest_path)
        .expect("manifest.toml must appear in report");
    assert_eq!(manifest_entry.action, ScaffoldAction::Skipped);

    let actual = s
        .get(&manifest_path)
        .expect("file must still be in scaffolder");
    assert_eq!(
        actual, original_content,
        "pre-existing file content must not be overwritten"
    );
}

#[test]
fn fs_scaffolder_rejects_absolute_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let s = FsScaffolder::new(tmp.path().to_path_buf());
    let err = s
        .write(Path::new("/tmp/escaped.txt"), b"x")
        .expect_err("absolute path must be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn fs_scaffolder_rejects_parent_dir_traversal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let s = FsScaffolder::new(tmp.path().to_path_buf());
    let err = s
        .write(Path::new("../escaped.txt"), b"x")
        .expect_err("parent-dir traversal must be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn fs_scaffolder_create_new_skips_existing_file_silently() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let s = FsScaffolder::new(tmp.path().to_path_buf());
    // First write succeeds.
    s.write(Path::new("foo.txt"), b"original")
        .expect("first write");
    // Second write must not clobber the original.
    s.write(Path::new("foo.txt"), b"replacement")
        .expect("second write should be silent Ok");
    let content = std::fs::read(tmp.path().join("foo.txt")).expect("read");
    assert_eq!(
        content, b"original",
        "FsScaffolder::write must not overwrite an existing file"
    );
}

#[test]
fn each_embedded_file_is_non_empty_after_write() {
    let s = InMemoryScaffolder::new();
    let report = scaffold_templates_dir(&s).expect("scaffold must not error");

    for entry in &report.files {
        // .gitkeep is intentionally empty; every other file must have content.
        if entry.path.ends_with(".gitkeep") {
            continue;
        }
        let content = s
            .get(&entry.path)
            .unwrap_or_else(|| panic!("file not found in scaffolder: {}", entry.path.display()));
        assert!(
            !content.is_empty(),
            "embedded file {} must not be empty — check the include_bytes! path",
            entry.path.display()
        );
    }
}
