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

use std::path::Path;

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;

const TEMPLATES_ROOT: &str = ".maestro/templates";

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
        super::read_sandboxed(Path::new(TEMPLATES_ROOT), path)
    }

    fn subagent_list(&self) -> Result<String, TemplateError> {
        super::subagent_list::load_subagent_list_markdown()
    }

    fn skill_link(&self, name: &str) -> Result<String, TemplateError> {
        super::read_skill_body(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn skill_link_inlines_existing_skill_body_verbatim() {
        let out = CodexRules.skill_link("project-patterns").expect("ok");
        assert!(
            out.contains("project-patterns") || out.contains("Maestro"),
            "expected SKILL.md content, got: {out:.120}"
        );
    }

    #[test]
    fn skill_link_missing_returns_file_missing() {
        let err = CodexRules
            .skill_link("definitely-not-a-real-skill-xyz")
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
    fn include_delegates_to_sandboxed_reader() {
        let err = CodexRules.include(Path::new("../Cargo.toml")).unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn rules_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CodexRules>();
    }
}
