//! Issue #758 — `{{VCS_PROVIDER_CMD}}` placeholder per-provider substitution.
//!
//! Proves the placeholder substitutes the per-provider CLI string in rendered
//! canonical commands. Today three real providers return `gh`; a custom rules
//! impl in this test proves a different CLI (e.g. `az repos` for Azure DevOps)
//! is wired through the same code path with zero canonical edits.

use std::path::Path;

use maestro::agent_provider::ClaudeProvider;
use maestro::templates::TemplateError;
use maestro::templates::provider_rules::TemplateProviderRules;
use maestro::templates::{render_command_for_rules, render_for_provider};

#[test]
fn plan_feature_render_for_claude_substitutes_gh() {
    let rendered = render_for_provider(&ClaudeProvider::default(), "plan-feature")
        .expect("plan-feature render must succeed for ClaudeProvider");
    assert!(
        rendered.contains("gh api repos/<owner>/<repo>/milestones"),
        "expected `gh api repos/...` after VCS substitution, got:\n{rendered}"
    );
    assert!(
        !rendered.contains("{{VCS_PROVIDER_CMD}}"),
        "unresolved VCS_PROVIDER_CMD placeholder in claude render"
    );
}

#[test]
fn plan_feature_render_with_azure_devops_rules_substitutes_az_repos() {
    let rendered = render_command_for_rules(
        Path::new(".maestro/templates"),
        &AzureDevOpsRules,
        "plan-feature",
    )
    .expect("plan-feature render must succeed for AzureDevOpsRules");
    assert!(
        rendered.contains("az repos api repos/<owner>/<repo>/milestones"),
        "expected `az repos api repos/...` after VCS substitution, got:\n{rendered}"
    );
    assert!(
        !rendered.contains("{{VCS_PROVIDER_CMD}}"),
        "unresolved VCS_PROVIDER_CMD placeholder in azuredevops render"
    );
}

#[test]
fn plan_feature_renders_with_zero_unresolved_placeholders_for_each_rules_shape() {
    let claude_out =
        render_for_provider(&ClaudeProvider::default(), "plan-feature").expect("claude render");
    assert!(
        !claude_out.contains("{{"),
        "claude render has unresolved `{{`"
    );
    assert!(
        !claude_out.contains("}}"),
        "claude render has unresolved `}}`"
    );

    let az_out = render_command_for_rules(
        Path::new(".maestro/templates"),
        &AzureDevOpsRules,
        "plan-feature",
    )
    .expect("azure render");
    assert!(!az_out.contains("{{"), "azure render has unresolved `{{`");
    assert!(!az_out.contains("}}"), "azure render has unresolved `}}`");
}

/// Rules-only stub mirroring how a real Azure DevOps provider would expose
/// `az repos` as its VCS CLI. Every other placeholder delegates to a
/// codex-equivalent shape so the render does not fail-closed on
/// SUBAGENT_LIST / INCLUDE / SKILL.
struct AzureDevOpsRules;

impl TemplateProviderRules for AzureDevOpsRules {
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
        let full = Path::new(".maestro/templates").join(path);
        std::fs::read_to_string(&full).map_err(|source| TemplateError::Io { path: full, source })
    }

    fn subagent_list(&self) -> Result<String, TemplateError> {
        Ok("| Subagent | Purpose |\n|---|---|\n".to_string())
    }

    fn skill_link(&self, name: &str) -> Result<String, TemplateError> {
        Ok(format!("the `{name}` skill"))
    }

    fn vcs_provider_cmd(&self) -> Result<String, TemplateError> {
        Ok("az repos".to_string())
    }
}
