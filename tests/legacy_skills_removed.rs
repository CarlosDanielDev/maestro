//! Regression guard for issue #760: ensure the legacy Claude-Code-only
//! `/simplify` skill stays deprecated and absent from the repository.
//!
//! The canonical simplify *command* (`.maestro/templates/commands/simplify.md`
//! and its rendered artifact `.claude/commands/simplify.md`) is intentionally
//! left intact and is covered separately by `tests/templates_render.rs`.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn is_markdown(p: &Path) -> bool {
    p.extension().and_then(|s| s.to_str()) == Some("md")
}

fn scan_md_files(dir: &Path, needle: &str, recurse_subdirs: bool) -> Vec<String> {
    let mut offenders = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return offenders;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && recurse_subdirs {
            offenders.extend(scan_md_files(&path, needle, false));
        } else if is_markdown(&path) {
            let body = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            if body.contains(needle) {
                offenders.push(path.display().to_string());
            }
        }
    }
    offenders
}

#[test]
fn legacy_simplify_skill_dir_absent() {
    let p = repo_root().join(".claude/skills/simplify");
    assert!(
        !p.exists(),
        "legacy skill directory reappeared at {} \
         (#760: superseded by .maestro/templates/commands/simplify.md)",
        p.display()
    );
}

#[test]
fn no_agent_prompt_references_legacy_simplify_skill() {
    let needle = ".claude/skills/simplify";
    let offenders = scan_md_files(&repo_root().join(".claude/agents"), needle, false);
    assert!(
        offenders.is_empty(),
        "agent prompts reference deprecated legacy skill `{needle}`: {offenders:?}"
    );
}

#[test]
fn no_skill_file_references_legacy_simplify_skill() {
    let needle = ".claude/skills/simplify";
    let offenders = scan_md_files(&repo_root().join(".claude/skills"), needle, true);
    assert!(
        offenders.is_empty(),
        "skill files reference deprecated legacy skill `{needle}`: {offenders:?}"
    );
}

#[test]
fn claude_md_skill_registry_has_no_simplify_row() {
    let body = std::fs::read_to_string(repo_root().join(".claude/CLAUDE.md"))
        .expect("read .claude/CLAUDE.md");
    let bad: Vec<&str> = body
        .lines()
        .filter(|line| line.starts_with('|') && line.contains("| `simplify`"))
        .collect();
    assert!(
        bad.is_empty(),
        ".claude/CLAUDE.md skill registry contains a `simplify` row (#760): {bad:?}"
    );
}
