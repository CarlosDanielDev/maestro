//! Golden Rules entry-file sync (issue #839).
//!
//! `maestro sync-templates` reads `.maestro/templates/core/golden-rules.md`
//! and splices its body between `<!-- BEGIN GOLDEN-RULES … -->` /
//! `<!-- END GOLDEN-RULES -->` markers in each provider entry file declared
//! under `[golden_rules.targets]` in the manifest. Outside the markers the
//! file is owned by humans and preserved byte-for-byte. `--check` flags any
//! divergence. `scripts/check-rules-drift.sh` remains as a secondary safety
//! net for non-Rust CI lanes.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::{Path, PathBuf};

use crate::templates::TemplateError;
use crate::templates::manifest::Manifest;

use super::runner::{SyncFs, SyncTemplatesError};

pub const BEGIN_MARKER_PREFIX: &str = "<!-- BEGIN GOLDEN-RULES";
pub const END_MARKER_LINE: &str = "<!-- END GOLDEN-RULES -->";

#[derive(Debug, thiserror::Error)]
pub enum GoldenRulesError {
    #[error("entry file `{path}` is missing the `<!-- BEGIN GOLDEN-RULES … -->` marker")]
    BeginMarkerMissing { path: PathBuf },
    #[error("entry file `{path}` is missing the `<!-- END GOLDEN-RULES -->` marker")]
    EndMarkerMissing { path: PathBuf },
    #[error("entry file `{path}` has END marker before BEGIN marker")]
    MarkersOutOfOrder { path: PathBuf },
    #[error("entry file `{path}` has more than one BEGIN GOLDEN-RULES marker")]
    DuplicateBeginMarker { path: PathBuf },
}

/// Splice `canonical_body` into `current` between the BEGIN/END markers.
///
/// Returns the full new file content; the BEGIN/END marker lines are
/// preserved verbatim (including any parenthetical hint on the BEGIN line).
/// Idempotency: applying twice yields identical bytes — the caller compares
/// the returned content to `current` to detect "no-op" cases.
pub fn splice(
    current: &str,
    canonical_body: &str,
    path: &std::path::Path,
) -> Result<String, GoldenRulesError> {
    let lines: Vec<&str> = current.split_inclusive('\n').collect();

    let mut begin_indices: Vec<usize> = Vec::new();
    let mut end_index: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.starts_with(BEGIN_MARKER_PREFIX) {
            begin_indices.push(i);
        }
        if end_index.is_none() && trimmed == END_MARKER_LINE {
            end_index = Some(i);
        }
    }

    if begin_indices.is_empty() {
        return Err(GoldenRulesError::BeginMarkerMissing {
            path: path.to_path_buf(),
        });
    }
    if begin_indices.len() > 1 {
        return Err(GoldenRulesError::DuplicateBeginMarker {
            path: path.to_path_buf(),
        });
    }
    let begin_index = begin_indices[0];
    let Some(end_index) = end_index else {
        return Err(GoldenRulesError::EndMarkerMissing {
            path: path.to_path_buf(),
        });
    };
    if end_index <= begin_index {
        return Err(GoldenRulesError::MarkersOutOfOrder {
            path: path.to_path_buf(),
        });
    }

    let mut out = String::with_capacity(current.len() + canonical_body.len());
    for line in &lines[..=begin_index] {
        out.push_str(line);
    }
    out.push_str(canonical_body);
    if !canonical_body.ends_with('\n') {
        out.push('\n');
    }
    for line in &lines[end_index..] {
        out.push_str(line);
    }
    Ok(out)
}

/// One spliced entry-file plan: target id, destination path, full new content.
pub struct EntryPlan {
    pub target_id: String,
    pub entry_path: PathBuf,
    pub content: String,
}

const TEMPLATES_ROOT: &str = ".maestro/templates";

/// Read the manifest and canonical body through `fs`, then splice the body
/// into every declared entry file. Gracefully no-ops when the manifest or
/// canonical body is absent (lets pre-existing tests with sparse FakeFs
/// seeds keep working).
pub fn build_entry_plans(
    repo_root: &Path,
    fs: &dyn SyncFs,
    provider_filter: Option<&str>,
) -> Result<Vec<EntryPlan>, SyncTemplatesError> {
    let manifest_path = repo_root.join(TEMPLATES_ROOT).join("manifest.toml");
    let Some(manifest) = load_manifest(&manifest_path, fs)? else {
        return Ok(Vec::new());
    };
    let Some(gr) = manifest.golden_rules() else {
        return Ok(Vec::new());
    };
    let canonical_path = repo_root.join(TEMPLATES_ROOT).join(&gr.source);
    let Some(canonical) = read_utf8_or_skip(&canonical_path, fs)? else {
        return Ok(Vec::new());
    };

    let mut plans = Vec::new();
    for target in &gr.targets {
        if let Some(filter) = provider_filter
            && filter != target.id
        {
            continue;
        }
        let entry_path = repo_root.join(&target.entry_file);
        let Some(current) = read_utf8_or_skip(&entry_path, fs)? else {
            continue;
        };
        let content = splice(&current, &canonical, &entry_path)?;
        plans.push(EntryPlan {
            target_id: target.id.clone(),
            entry_path,
            content,
        });
    }
    Ok(plans)
}

fn load_manifest(path: &Path, fs: &dyn SyncFs) -> Result<Option<Manifest>, SyncTemplatesError> {
    let Some(text) = read_utf8_or_skip(path, fs)? else {
        return Ok(None);
    };
    toml::from_str(&text)
        .map(Some)
        .map_err(|e| SyncTemplatesError::ManifestLoad {
            path: path.to_path_buf(),
            source: Box::new(TemplateError::ManifestParse {
                path: path.to_path_buf(),
                source: e,
            }),
        })
}

fn read_utf8_or_skip(path: &Path, fs: &dyn SyncFs) -> Result<Option<String>, SyncTemplatesError> {
    match fs.read(path) {
        Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(SyncTemplatesError::Write {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn path() -> &'static Path {
        Path::new("/test/entry.md")
    }

    #[test]
    fn splice_replaces_block_between_markers() {
        let current = "\
header line
<!-- BEGIN GOLDEN-RULES -->
old body
gets replaced
<!-- END GOLDEN-RULES -->
footer line
";
        let canonical = "fresh canonical body";
        let out = splice(current, canonical, path()).expect("ok");
        assert!(out.contains("header line"));
        assert!(out.contains("<!-- BEGIN GOLDEN-RULES -->"));
        assert!(out.contains("fresh canonical body"));
        assert!(out.contains("<!-- END GOLDEN-RULES -->"));
        assert!(out.contains("footer line"));
        assert!(!out.contains("old body"));
        assert!(!out.contains("gets replaced"));
    }

    #[test]
    fn splice_is_idempotent() {
        let canonical = "body\n";
        let once = splice(
            "x\n<!-- BEGIN GOLDEN-RULES -->\nstale\n<!-- END GOLDEN-RULES -->\ny\n",
            canonical,
            path(),
        )
        .expect("ok");
        let twice = splice(&once, canonical, path()).expect("ok");
        assert_eq!(once, twice);
    }

    #[test]
    fn splice_preserves_begin_marker_line_verbatim_with_parenthetical() {
        let current = "\
<!-- BEGIN GOLDEN-RULES (do not edit — see canonical) -->
old
<!-- END GOLDEN-RULES -->
";
        let out = splice(current, "new\n", path()).expect("ok");
        assert!(
            out.contains("<!-- BEGIN GOLDEN-RULES (do not edit — see canonical) -->"),
            "BEGIN line must be preserved verbatim; got:\n{out}"
        );
    }

    #[test]
    fn splice_missing_begin_returns_error() {
        let current = "no markers here\n<!-- END GOLDEN-RULES -->\n";
        match splice(current, "x", path()) {
            Err(GoldenRulesError::BeginMarkerMissing { .. }) => {}
            other => panic!("expected BeginMarkerMissing, got {other:?}"),
        }
    }

    #[test]
    fn splice_missing_end_returns_error() {
        let current = "<!-- BEGIN GOLDEN-RULES -->\nbody\n";
        match splice(current, "x", path()) {
            Err(GoldenRulesError::EndMarkerMissing { .. }) => {}
            other => panic!("expected EndMarkerMissing, got {other:?}"),
        }
    }

    #[test]
    fn splice_end_before_begin_returns_error() {
        let current = "<!-- END GOLDEN-RULES -->\n<!-- BEGIN GOLDEN-RULES -->\n";
        match splice(current, "x", path()) {
            Err(GoldenRulesError::MarkersOutOfOrder { .. }) => {}
            other => panic!("expected MarkersOutOfOrder, got {other:?}"),
        }
    }

    #[test]
    fn splice_duplicate_begin_returns_error() {
        let current = "\
<!-- BEGIN GOLDEN-RULES -->
body
<!-- BEGIN GOLDEN-RULES -->
more body
<!-- END GOLDEN-RULES -->
";
        match splice(current, "x", path()) {
            Err(GoldenRulesError::DuplicateBeginMarker { .. }) => {}
            other => panic!("expected DuplicateBeginMarker, got {other:?}"),
        }
    }

    #[test]
    fn splice_canonical_without_trailing_newline_gets_one_added() {
        let current = "<!-- BEGIN GOLDEN-RULES -->\nold\n<!-- END GOLDEN-RULES -->\n";
        let out = splice(current, "no trailing newline", path()).expect("ok");
        assert!(out.contains("no trailing newline\n<!-- END GOLDEN-RULES -->"));
    }
}
