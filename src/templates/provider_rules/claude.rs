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

use std::path::Path;

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
        super::read_sandboxed(Path::new(TEMPLATES_ROOT), path)
    }

    fn subagent_list(&self) -> Result<String, TemplateError> {
        super::subagent_list::load_subagent_list_markdown()
    }

    fn skill_link(&self, name: &str) -> Result<String, TemplateError> {
        Ok(format!(
            "the `{name}` skill (.claude/skills/{name}/SKILL.md)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn include_delegates_to_sandboxed_reader() {
        let err = ClaudeRules.include(Path::new("../Cargo.toml")).unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn rules_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClaudeRules>();
    }
}
