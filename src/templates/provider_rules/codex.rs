//! Codex CLI provider rules for the canonical templates engine.
//!
//! Codex has no Task tool, no skills system, and no project-level slash-
//! command discovery directory. Project-scoped customisation funnels through
//! `AGENTS.md` (see <https://developers.openai.com/codex/guides/agents-md>).
//! Custom prompts under `~/.codex/prompts/` are deprecated and user-home-only.
//!
//! Consequences for the rule translations below:
//! - `target_dir()` returns `None` — there is no `.codex/commands/` analogue
//!   to Claude's `.claude/commands/`. A future ticket will merge rendered
//!   command bodies into `AGENTS.md`; that integration is out of scope here.
//! - `invoke_subagent` inline-expands ("## Sub-task: <name>\n\n<prompt>")
//!   instead of referencing a Task tool that Codex does not have.
//! - `skill_link` inlines the skill body verbatim. Codex won't follow a
//!   `.claude/skills/...` path, so the *content* is loaded at render time.
//! - `subagent_list` returns a plain Markdown table without links.
//! - `hook_gate` is provider-neutral (`bash .maestro/hooks/<script>`).
//! - `include` mirrors Claude's sandboxed reader — paths resolved under
//!   `.maestro/templates/`, parent-dir / absolute paths rejected.
//!
//! ## Security note
//!
//! `skill_link` is **inline** for Codex (unlike Claude which emits a path
//! reference). Anything written into `.claude/skills/<name>/SKILL.md` ships
//! verbatim into every rendered Codex command document. Treat `.claude/skills`
//! as part of the prompt surface when reviewing PRs that touch those files.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::{Component, Path};

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;

const TEMPLATES_ROOT: &str = ".maestro/templates";
const SKILLS_ROOT: &str = ".claude/skills";

#[derive(Debug, Default)]
pub struct CodexRules;

/// Shared `'static` reference to the [`CodexRules`] singleton.
///
/// Mirrors [`crate::templates::provider_rules::claude_rules`] so every
/// `AgentProvider::template_rules()` impl reads as a single delegation.
pub fn codex_rules() -> &'static dyn TemplateProviderRules {
    static RULES: CodexRules = CodexRules;
    &RULES
}

impl TemplateProviderRules for CodexRules {
    fn target_dir(&self) -> Option<&'static Path> {
        None
    }

    fn invoke_subagent(&self, name: &str, prompt: &str) -> Result<String, TemplateError> {
        Ok(format!("## Sub-task: {name}\n\n{prompt}"))
    }

    fn hook_gate(&self, script: &str, args: &str) -> Result<String, TemplateError> {
        if args.is_empty() {
            Ok(format!("bash .maestro/hooks/{script}"))
        } else {
            Ok(format!("bash .maestro/hooks/{script} {args}"))
        }
    }

    fn include(&self, path: &Path) -> Result<String, TemplateError> {
        read_sandboxed(Path::new(TEMPLATES_ROOT), path)
    }

    fn subagent_list(&self) -> Result<String, TemplateError> {
        Ok(SUBAGENT_LIST_MARKDOWN.to_string())
    }

    fn skill_link(&self, name: &str) -> Result<String, TemplateError> {
        let skill_path = format!("{name}/SKILL.md");
        read_sandboxed(Path::new(SKILLS_ROOT), Path::new(&skill_path))
    }
}

fn read_sandboxed(root: &Path, path: &Path) -> Result<String, TemplateError> {
    let display_path = path.to_string_lossy().into_owned();
    let root_display = root.to_string_lossy().into_owned();
    let escape = || TemplateError::SandboxEscape {
        path: display_path.clone(),
        root: root_display.clone(),
    };
    if path.is_absolute() {
        return Err(escape());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(escape());
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
    fn target_dir_is_none_codex_has_no_project_slash_command_dir() {
        assert!(CodexRules.target_dir().is_none());
    }

    #[test]
    fn invoke_subagent_inlines_as_sub_task_heading() {
        let out = CodexRules.invoke_subagent("architect", "Do X").expect("ok");
        assert_eq!(out, "## Sub-task: architect\n\nDo X");
    }

    #[test]
    fn invoke_subagent_with_empty_prompt_keeps_trailing_blank_line() {
        let out = CodexRules.invoke_subagent("qa", "").expect("ok");
        assert_eq!(out, "## Sub-task: qa\n\n");
    }

    #[test]
    fn hook_gate_drops_trailing_space_when_args_empty() {
        let out = CodexRules.hook_gate("preflight.sh", "").expect("ok");
        assert_eq!(out, "bash .maestro/hooks/preflight.sh");
    }

    #[test]
    fn hook_gate_includes_args_when_non_empty() {
        let out = CodexRules
            .hook_gate("implement-gates.sh", "$ISSUE_NUMBER")
            .expect("ok");
        assert_eq!(out, "bash .maestro/hooks/implement-gates.sh $ISSUE_NUMBER");
    }

    #[test]
    fn subagent_list_is_markdown_table_with_known_subagents() {
        let out = CodexRules.subagent_list().expect("ok");
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
    fn skill_link_inlines_skill_body_verbatim() {
        let root = manifest_dir().join(".claude/skills");
        let out = read_sandboxed(&root, Path::new("project-patterns/SKILL.md")).expect("ok");
        assert!(
            out.contains("project-patterns") || out.contains("Maestro"),
            "expected SKILL.md content, got: {out:.120}"
        );
    }

    #[test]
    fn skill_link_missing_returns_file_missing() {
        let root = manifest_dir().join(".claude/skills");
        let err = read_sandboxed(&root, Path::new("definitely-not-a-real-skill-xyz/SKILL.md"))
            .unwrap_err();
        assert!(matches!(err, TemplateError::FileMissing { .. }), "{err:?}");
    }

    #[test]
    fn skill_link_rejects_parent_dir_traversal_via_name() {
        let err = CodexRules.skill_link("../etc").unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn include_reads_existing_core_premises() {
        let root = manifest_dir().join(".maestro/templates");
        let out = read_sandboxed(&root, Path::new("core/premises.md")).expect("ok");
        assert!(
            out.contains("YOU ARE THE ONLY AGENT THAT WRITES CODE"),
            "unexpected content: {out:.120}"
        );
    }

    #[test]
    fn include_rejects_parent_dir_traversal() {
        let root = manifest_dir().join(".maestro/templates");
        let err = read_sandboxed(&root, Path::new("../Cargo.toml")).unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn include_rejects_absolute_path() {
        let root = manifest_dir().join(".maestro/templates");
        let err = read_sandboxed(&root, Path::new("/etc/passwd")).unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn include_missing_file_returns_file_missing() {
        let root = manifest_dir().join(".maestro/templates");
        let err = read_sandboxed(&root, Path::new("core/does-not-exist.md")).unwrap_err();
        assert!(matches!(err, TemplateError::FileMissing { .. }), "{err:?}");
    }

    #[test]
    fn rules_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CodexRules>();
    }
}
