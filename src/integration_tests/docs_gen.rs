//! Drift guard for `docs/configuration.md` against the config schema (#717).
//!
//! Three tests:
//! - `docs_gen_no_drift`: regenerate in-memory and byte-compare against the
//!   checked-in file. Fails with the regeneration command if drift is found.
//! - `docs_gen_regenerate` (`#[ignore]`): rewrites the file on disk; run via
//!   `bash scripts/regenerate-docs.sh`.
//! - `docs_gen_all_schema_tables_have_markers`: warns when a top-level
//!   `TableSchema` lacks a `<!-- BEGIN AUTOGEN:NAME -->` marker in the doc.

use std::path::PathBuf;

use crate::config::schema::{docs_render, schema_for_config};

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("configuration.md")
}

fn first_n_line_diff(a: &str, b: &str, n: usize) -> String {
    let al: Vec<&str> = a.lines().collect();
    let bl: Vec<&str> = b.lines().collect();
    let max = al.len().max(bl.len());
    let mut out = String::new();
    let mut emitted = 0;
    for i in 0..max {
        if emitted >= n {
            out.push_str("…(truncated)\n");
            break;
        }
        let av = al.get(i).copied();
        let bv = bl.get(i).copied();
        match (av, bv) {
            (Some(x), Some(y)) if x != y => {
                out.push_str(&format!("L{:>4} - {x}\n", i + 1));
                out.push_str(&format!("L{:>4} + {y}\n", i + 1));
                emitted += 1;
            }
            (Some(x), None) => {
                out.push_str(&format!("L{:>4} - {x}\n", i + 1));
                emitted += 1;
            }
            (None, Some(y)) => {
                out.push_str(&format!("L{:>4} + {y}\n", i + 1));
                emitted += 1;
            }
            _ => {}
        }
    }
    out
}

#[test]
fn docs_gen_no_drift() {
    let path = doc_path();
    let existing =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let regenerated = docs_render::regenerate(&existing, schema_for_config())
        .unwrap_or_else(|e| panic!("regenerate failed: {e}"));

    if regenerated == existing {
        return;
    }

    let diff = first_n_line_diff(&existing, &regenerated, 20);
    panic!(
        "docs/configuration.md is out of sync with the config schema.\n\
         Fix: run `bash scripts/regenerate-docs.sh` and commit the result.\n\
         \n\
         First differing lines (- committed, + regenerated):\n{diff}"
    );
}

#[test]
#[ignore = "writes to disk; run via scripts/regenerate-docs.sh"]
fn docs_gen_regenerate() {
    let path = doc_path();
    let existing =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let regenerated = docs_render::regenerate(&existing, schema_for_config())
        .unwrap_or_else(|e| panic!("regenerate failed: {e}"));

    std::fs::write(&path, &regenerated).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    eprintln!("Regenerated {}", path.display());
}

#[test]
fn docs_gen_all_schema_tables_have_markers() {
    // Schema tables expected to be wrapped in AUTOGEN markers. Sections where
    // the schema is incomplete relative to docs/configuration.md are listed
    // in `SCHEMA_BACKFILL_PENDING` until a follow-up backfills them.
    //
    // #788 closed the project/sessions/review/tui/tui.theme/turboquant/
    // concurrency backfill. agents/modes/teams remain deferred because
    // their hand-written sections in docs/configuration.md include
    // provider-specific notes (HTTP-vs-subprocess defaults, model
    // fallbacks, `maestro doctor` checks, team-preset extends/bindings
    // semantics) the schema does not capture; autogen lands alongside the
    // docs-render refactor for `FlattenedMap`.
    const SCHEMA_BACKFILL_PENDING: &[&str] = &["agents", "modes", "teams"];

    let path = doc_path();
    let existing =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let mut missing: Vec<&'static str> = Vec::new();
    for table in schema_for_config() {
        if SCHEMA_BACKFILL_PENDING.contains(&table.name) {
            continue;
        }
        let marker = format!("<!-- BEGIN AUTOGEN:{} -->", table.name);
        if !existing.contains(&marker) {
            missing.push(table.name);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/configuration.md missing AUTOGEN markers for schema tables: {missing:?}.\n\
         Add `<!-- BEGIN AUTOGEN:NAME --> ... <!-- END AUTOGEN:NAME -->` blocks for each, \
         then run `bash scripts/regenerate-docs.sh`."
    );
}
