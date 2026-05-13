//! Claude Code provider rules for the canonical templates engine.
//!
//! Translates the five placeholder kinds into the text shapes consumed by
//! `.claude/commands/*.md`. `.maestro/templates/` is the single source of
//! truth; rendered output is committed as a generated artifact under
//! `.claude/commands/` and guarded by `tests/templates_render.rs`.
//!
//! Zero-state singleton — `ClaudeProvider::template_rules()` returns a
//! reference to a `static` instance.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::{Component, Path};

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;

const TARGET_DIR: &str = ".claude/commands";
const TEMPLATES_ROOT: &str = ".maestro/templates";

#[derive(Debug, Default)]
pub struct ClaudeRules;

/// Shared `'static` reference to the [`ClaudeRules`] singleton.
///
/// Mirrors [`crate::templates::provider_rules::null_rules`] so every
/// `AgentProvider::template_rules()` impl reads as a single delegation.
pub fn claude_rules() -> &'static dyn TemplateProviderRules {
    static RULES: ClaudeRules = ClaudeRules;
    &RULES
}

impl TemplateProviderRules for ClaudeRules {
    fn target_dir(&self) -> Option<&'static Path> {
        Some(Path::new(TARGET_DIR))
    }

    fn invoke_subagent(&self, name: &str, prompt: &str) -> Result<String, TemplateError> {
        Ok(format!(
            "Use the Task tool to launch the `subagent-{name}` subagent with the prompt below.\n\n{prompt}"
        ))
    }

    fn hook_gate(&self, script: &str, args: &str) -> Result<String, TemplateError> {
        if args.is_empty() {
            Ok(format!("bash .claude/hooks/{script}"))
        } else {
            Ok(format!("bash .claude/hooks/{script} {args}"))
        }
    }

    fn include(&self, path: &Path) -> Result<String, TemplateError> {
        read_include(Path::new(TEMPLATES_ROOT), path)
    }

    fn subagent_list(&self) -> Result<String, TemplateError> {
        Ok(SUBAGENT_LIST_MARKDOWN.to_string())
    }

    fn skill_link(&self, name: &str) -> Result<String, TemplateError> {
        Ok(format!(
            "the `{name}` skill (.claude/skills/{name}/SKILL.md)"
        ))
    }
}

fn read_include(root: &Path, path: &Path) -> Result<String, TemplateError> {
    let display_path = path.to_string_lossy().into_owned();
    let root_display = root.to_string_lossy().into_owned();
    let escape = || TemplateError::SandboxEscape {
        path: display_path.clone(),
        root: root_display.clone(),
    };
    if path.is_absolute() {
        return Err(escape());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(escape()),
        }
    }
    let full = root.join(path);
    let canonical_root = std::fs::canonicalize(root).map_err(|source| TemplateError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let canonical_full = std::fs::canonicalize(&full).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => TemplateError::FileMissing { path: full.clone() },
        _ => TemplateError::Io {
            path: full.clone(),
            source,
        },
    })?;
    if !canonical_full.starts_with(&canonical_root) {
        return Err(escape());
    }
    std::fs::read_to_string(&canonical_full).map_err(|source| TemplateError::Io {
        path: canonical_full,
        source,
    })
}

const SUBAGENT_LIST_MARKDOWN: &str = "\
| Subagent | Purpose |
|----------|---------|
| `subagent-gatekeeper` | DOR, blockers, and API-contract gate for `/implement` |
| `subagent-architect` | Architecture design and implementation planning |
| `subagent-qa` | QA engineering, test design, quality gates |
| `subagent-security-analyst` | Security review (OWASP Top 10) |
| `subagent-docs-analyst` | Documentation management (only subagent allowed to write `.md`) |
| `subagent-master-planner` | System architecture planning, ADRs |
| `subagent-idea-triager` | Idea-inbox triage gate (5-question honesty check)";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn manifest_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn target_dir_is_dot_claude_commands() {
        assert_eq!(
            ClaudeRules.target_dir(),
            Some(Path::new(".claude/commands"))
        );
    }

    #[test]
    fn invoke_subagent_prefixes_name_and_appends_prompt() {
        let out = ClaudeRules
            .invoke_subagent("architect", "Do X")
            .expect("ok");
        assert_eq!(
            out,
            "Use the Task tool to launch the `subagent-architect` subagent with the prompt below.\n\nDo X"
        );
    }

    #[test]
    fn hook_gate_drops_trailing_space_when_args_empty() {
        let out = ClaudeRules.hook_gate("preflight.sh", "").expect("ok");
        assert_eq!(out, "bash .claude/hooks/preflight.sh");
    }

    #[test]
    fn hook_gate_includes_args_when_non_empty() {
        let out = ClaudeRules
            .hook_gate("implement-gates.sh", "$ISSUE_NUMBER")
            .expect("ok");
        assert_eq!(out, "bash .claude/hooks/implement-gates.sh $ISSUE_NUMBER");
    }

    #[test]
    fn skill_link_renders_path_phrase() {
        let out = ClaudeRules.skill_link("project-patterns").expect("ok");
        assert_eq!(
            out,
            "the `project-patterns` skill (.claude/skills/project-patterns/SKILL.md)"
        );
    }

    #[test]
    fn subagent_list_is_markdown_table_with_known_subagents() {
        let out = ClaudeRules.subagent_list().expect("ok");
        assert!(out.starts_with("| Subagent | Purpose |"), "{out}");
        for slug in [
            "subagent-gatekeeper",
            "subagent-architect",
            "subagent-qa",
            "subagent-security-analyst",
            "subagent-docs-analyst",
            "subagent-master-planner",
            "subagent-idea-triager",
        ] {
            assert!(out.contains(slug), "missing `{slug}` in: {out}");
        }
    }

    #[test]
    fn include_reads_existing_core_premises() {
        let root = manifest_dir().join(".maestro/templates");
        let out = read_include(&root, Path::new("core/premises.md")).expect("ok");
        assert!(
            out.contains("YOU ARE THE ONLY AGENT THAT WRITES CODE"),
            "unexpected content: {out:.120}"
        );
    }

    #[test]
    fn include_rejects_parent_dir_traversal() {
        let root = manifest_dir().join(".maestro/templates");
        let err = read_include(&root, Path::new("../Cargo.toml")).unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn include_rejects_absolute_path() {
        let root = manifest_dir().join(".maestro/templates");
        let err = read_include(&root, Path::new("/etc/passwd")).unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn include_missing_file_returns_file_missing() {
        let root = manifest_dir().join(".maestro/templates");
        let err = read_include(&root, Path::new("core/does-not-exist.md")).unwrap_err();
        assert!(matches!(err, TemplateError::FileMissing { .. }), "{err:?}");
    }

    #[test]
    fn rules_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClaudeRules>();
    }
}
