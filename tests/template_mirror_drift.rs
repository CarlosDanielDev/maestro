//! Drift guard: the canonical templates at `.maestro/templates/` and the
//! embedded scaffolding source at `template/.maestro/templates/` MUST stay
//! byte-identical. The init scaffolder embeds the latter; the sync-templates
//! flow renders the former. Any divergence breaks `maestro init` correctness.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_tree(base: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let walker = walkdir::WalkDir::new(base).sort_by_file_name();
    for entry in walker {
        let entry = entry.unwrap_or_else(|e| panic!("walk error under {}: {e}", base.display()));
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(base)
            .expect("strip_prefix")
            .to_path_buf();
        let bytes = std::fs::read(entry.path())
            .unwrap_or_else(|e| panic!("read {}: {e}", entry.path().display()));
        out.insert(rel, bytes);
    }
    out
}

#[test]
fn template_mirror_same_paths() {
    let root = manifest_dir();
    let canonical = collect_tree(&root.join(".maestro/templates"));
    let mirror = collect_tree(&root.join("template/.maestro/templates"));

    let canonical_keys: Vec<&PathBuf> = canonical.keys().collect();
    let mirror_keys: Vec<&PathBuf> = mirror.keys().collect();

    assert_eq!(
        canonical_keys, mirror_keys,
        "path mismatch between .maestro/templates and template/.maestro/templates.\n\
         Fix: cp -a .maestro/templates/. template/.maestro/templates/"
    );
}

#[test]
fn template_mirror_byte_equal() {
    let root = manifest_dir();
    let canonical = collect_tree(&root.join(".maestro/templates"));
    let mirror = collect_tree(&root.join("template/.maestro/templates"));

    for (rel, canonical_bytes) in &canonical {
        let mirror_bytes = mirror.get(rel).unwrap_or_else(|| {
            panic!(
                "file missing from mirror: {rel}\nFix: cp .maestro/templates/{rel} template/.maestro/templates/{rel}",
                rel = rel.display()
            )
        });
        assert_eq!(
            canonical_bytes,
            mirror_bytes,
            "byte mismatch for {rel}.\nFix: cp .maestro/templates/{rel} template/.maestro/templates/{rel}",
            rel = rel.display()
        );
    }
}
