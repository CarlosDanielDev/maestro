//! HTTP-generic provider rules for the canonical templates engine.
//!
//! Shared by HTTP-transport providers (Ollama, MiniMax) and by Qwen, none of
//! which have a Claude-style `.<provider>/commands/` slash-command discovery
//! directory. The rendered output is intended to be cached on disk by
//! `maestro sync-templates` (#707) and appended at session spawn as a
//! system-prompt appendix (#708) — there is no on-disk `target_dir`.
//!
//! Consequences for the rule translations below:
//! - `target_dir()` returns `None`. HTTP providers do not consume `.md` files
//!   from a discovery directory; rendered output is injected at runtime.
//! - `invoke_subagent` inline-expands ("## Sub-task: <name>\n\n<prompt>"),
//!   matching Codex — there is no Task-tool analogue.
//! - `hook_gate` renders **instruction text**, not an executable command,
//!   because the agent process cannot shell out. The orchestrator (Claude
//!   Code on the human side) reads the instruction and runs the hook.
//! - `include` mirrors Codex's sandboxed reader, rooted at `.maestro/templates/`.
//! - `subagent_list` returns the same Markdown table as Codex (no links).
//! - `skill_link` inlines the skill body verbatim, like Codex.
//!
//! ## Security note
//!
//! `skill_link` is **inline** here (same as Codex). Anything written into
//! `.claude/skills/<name>/SKILL.md` ships verbatim into every rendered HTTP
//! command document. Treat `.claude/skills` as part of the prompt surface.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::Path;

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;

const TEMPLATES_ROOT: &str = ".maestro/templates";

#[derive(Debug, Default)]
pub struct HttpGenericRules;

/// Shared `'static` reference to the [`HttpGenericRules`] singleton.
///
/// Mirrors [`crate::templates::provider_rules::codex_rules`] so every
/// `AgentProvider::template_rules()` impl reads as a single delegation.
pub fn http_generic_rules() -> &'static dyn TemplateProviderRules {
    static RULES: HttpGenericRules = HttpGenericRules;
    &RULES
}

impl TemplateProviderRules for HttpGenericRules {
    fn target_dir(&self) -> Option<&'static Path> {
        None
    }

    fn invoke_subagent(&self, name: &str, prompt: &str) -> Result<String, TemplateError> {
        Ok(format!("## Sub-task: {name}\n\n{prompt}"))
    }

    fn hook_gate(&self, script: &str, args: &str) -> Result<String, TemplateError> {
        let suffix = if args.is_empty() {
            String::new()
        } else {
            format!(" {args}")
        };
        Ok(format!(
            "Before proceeding, the orchestrator MUST run: \
             `bash .maestro/hooks/{script}{suffix}` and verify exit 0."
        ))
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
    fn target_dir_is_none_http_providers_have_no_on_disk_discovery() {
        assert!(HttpGenericRules.target_dir().is_none());
    }

    #[test]
    fn invoke_subagent_inlines_as_sub_task_heading() {
        let out = HttpGenericRules
            .invoke_subagent("architect", "Do X")
            .expect("ok");
        assert_eq!(out, "## Sub-task: architect\n\nDo X");
    }

    #[test]
    fn invoke_subagent_with_empty_name_keeps_heading_prefix() {
        let out = HttpGenericRules.invoke_subagent("", "Do X").expect("ok");
        assert_eq!(out, "## Sub-task: \n\nDo X");
    }

    #[test]
    fn hook_gate_renders_instruction_text_not_shell_command() {
        let out = HttpGenericRules
            .hook_gate("implement-gates.sh", "$ISSUE_NUMBER")
            .expect("ok");
        assert!(
            out.starts_with("Before proceeding, the orchestrator MUST run:"),
            "unexpected hook_gate prefix: {out}"
        );
        assert!(out.ends_with("verify exit 0."), "{out}");
        assert!(
            !out.starts_with("bash "),
            "hook_gate must not render a raw shell command: {out}"
        );
    }

    #[test]
    fn hook_gate_drops_trailing_space_when_args_empty() {
        let out = HttpGenericRules.hook_gate("preflight.sh", "").expect("ok");
        assert_eq!(
            out,
            "Before proceeding, the orchestrator MUST run: \
             `bash .maestro/hooks/preflight.sh` and verify exit 0."
        );
    }

    #[test]
    fn hook_gate_instruction_contains_script_path() {
        let out = HttpGenericRules
            .hook_gate("implement-gates.sh", "$ISSUE_NUMBER")
            .expect("ok");
        assert!(
            out.contains("`bash .maestro/hooks/implement-gates.sh $ISSUE_NUMBER`"),
            "expected embedded command literal in: {out}"
        );
    }

    #[test]
    fn skill_link_inlines_existing_skill_body_verbatim() {
        let out = HttpGenericRules.skill_link("project-patterns").expect("ok");
        assert!(
            out.contains("project-patterns") || out.contains("Maestro"),
            "expected SKILL.md content, got: {out:.120}"
        );
    }

    #[test]
    fn skill_link_rejects_parent_dir_traversal_via_name() {
        let err = HttpGenericRules.skill_link("../etc").unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn skill_link_with_empty_name_is_rejected_as_sandbox_escape() {
        // Empty `name` produces path "/SKILL.md", which is absolute on Unix
        // and rejected by the sandbox before any filesystem lookup.
        let err = HttpGenericRules.skill_link("").unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn include_delegates_to_sandboxed_reader() {
        let err = HttpGenericRules
            .include(Path::new("../Cargo.toml"))
            .unwrap_err();
        assert!(
            matches!(err, TemplateError::SandboxEscape { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn rules_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<HttpGenericRules>();
    }
}
