//! Shared `{{SUBAGENT_LIST}}` renderer.
//!
//! Claude, Codex, and HTTP-generic all emit the identical subagent-registry
//! Markdown table. The body is now derived from the `[[subagents]]` array in
//! `.maestro/templates/manifest.toml`; the on-disk `.claude/agents/` registry
//! is the drift detector (see `tests/subagent_manifest_drift.rs`).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::Path;

use crate::templates::{Manifest, ManifestSubagent, TemplateError};

const MANIFEST_PATH: &str = ".maestro/templates/manifest.toml";

/// Render the canonical subagent-registry Markdown table.
///
/// Order is preserved from the manifest (pipeline order, not alphabetical).
/// Rows 1..n-1 end with ` |`; the last row drops the trailing pipe to match
/// the legacy const that was committed under each provider before issue #728.
/// The string has no trailing newline.
pub(super) fn render_subagent_list_default(subagents: &[ManifestSubagent]) -> String {
    let mut out = String::from("| Subagent | Purpose |\n|----------|---------|");
    for sa in subagents {
        out.push('\n');
        out.push_str(&format!("| `{}` | {} |", sa.slug, sa.purpose));
    }
    if out.ends_with(" |") {
        out.truncate(out.len() - 2);
    }
    out
}

/// Load `.maestro/templates/manifest.toml` and return its subagent registry,
/// rendered as Markdown. The manifest is re-parsed on every call; the cost
/// (~50 µs per parse) is invisible at template-render cadence and avoids
/// threading a manifest handle through `TemplateProviderRules`. If render
/// becomes hot, cache via `OnceLock<Manifest>`.
pub(super) fn load_subagent_list_markdown() -> Result<String, TemplateError> {
    let manifest = Manifest::load(Path::new(MANIFEST_PATH))?;
    Ok(render_subagent_list_default(manifest.subagents()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(slug: &str, purpose: &str) -> ManifestSubagent {
        ManifestSubagent {
            slug: slug.to_string(),
            purpose: purpose.to_string(),
        }
    }

    #[test]
    fn empty_slice_returns_header_only() {
        let out = render_subagent_list_default(&[]);
        assert_eq!(out, "| Subagent | Purpose |\n|----------|---------|");
    }

    #[test]
    fn single_entry_omits_trailing_pipe() {
        let out = render_subagent_list_default(&[make("subagent-gatekeeper", "DOR gate")]);
        assert!(out.ends_with("| `subagent-gatekeeper` | DOR gate"), "{out}");
        assert!(!out.ends_with(" |"), "{out}");
        assert!(!out.ends_with('\n'), "{out}");
        assert_eq!(out.lines().count(), 3);
    }

    #[test]
    fn intermediate_rows_end_with_pipe_last_does_not() {
        let out = render_subagent_list_default(&[
            make("subagent-a", "first"),
            make("subagent-b", "second"),
        ]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[2], "| `subagent-a` | first |");
        assert_eq!(lines[3], "| `subagent-b` | second");
    }

    #[test]
    fn special_chars_pass_through_unchanged() {
        let out = render_subagent_list_default(&[
            make(
                "subagent-docs-analyst",
                "Documentation management (only subagent allowed to write `.md`)",
            ),
            make(
                "subagent-idea-triager",
                "Idea-inbox triage gate (5-question honesty check)",
            ),
        ]);
        assert!(out.contains("`.md`"), "{out}");
        assert!(out.contains("(5-question honesty check)"), "{out}");
    }

    #[test]
    fn order_preserved_from_input() {
        let out = render_subagent_list_default(&[make("subagent-z", "z"), make("subagent-a", "a")]);
        let z_pos = out.find("subagent-z").expect("z");
        let a_pos = out.find("subagent-a").expect("a");
        assert!(z_pos < a_pos, "{out}");
    }

    #[test]
    fn separator_row_has_fixed_width() {
        let out = render_subagent_list_default(&[make("subagent-x", "x")]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[1], "|----------|---------|");
    }

    #[test]
    fn load_subagent_list_markdown_against_live_manifest_is_byte_identical() {
        let out = load_subagent_list_markdown().expect("load");
        let expected = "\
| Subagent | Purpose |
|----------|---------|
| `subagent-gatekeeper` | DOR, blockers, and API-contract gate for `/implement` |
| `subagent-architect` | Architecture design and implementation planning |
| `subagent-qa` | QA engineering, test design, quality gates |
| `subagent-security-analyst` | Security review (OWASP Top 10) |
| `subagent-docs-analyst` | Documentation management (only subagent allowed to write `.md`) |
| `subagent-master-planner` | System architecture planning, ADRs |
| `subagent-idea-triager` | Idea-inbox triage gate (5-question honesty check)";
        assert_eq!(out, expected);
    }
}
