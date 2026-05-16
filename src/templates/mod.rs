//! Canonical template rendering engine.
//!
//! Renders provider-agnostic command specs under `.maestro/templates/commands/`
//! into provider-specific output. Fixed placeholder vocabulary (5 kinds):
//! `{{INVOKE_SUBAGENT}}`, `{{HOOK_GATE}}`, `{{INCLUDE}}`, `{{SUBAGENT_LIST}}`,
//! `{{SKILL}}`. Unknown placeholders fail closed — see [`TemplateError`].

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod error;
mod manifest;
pub mod provider_rules;
pub mod rendered_cache;
mod renderer;
#[cfg(test)]
pub(crate) mod test_fakes;

pub use error::TemplateError;
pub(crate) use manifest::{Manifest, ManifestSubagent};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use manifest::{ManifestMeta, ManifestPlaceholder, ManifestProvider};
#[cfg(test)]
pub(crate) use provider_rules::NullRules;
pub use provider_rules::{TemplateProviderRules, null_rules};
#[cfg(test)]
pub(crate) use rendered_cache::FakeRenderedStore;
#[allow(unused_imports)]
// Reason: DiskRenderedTemplateStore wired in setup once active_command is plumbed (#707)
pub use rendered_cache::{DiskRenderedTemplateStore, RenderedTemplateStore};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use renderer::{PlaceholderKind, Token, tokenize};

use std::path::{Path, PathBuf};

use crate::agent_provider::AgentProvider;

const DEFAULT_TEMPLATES_ROOT: &str = ".maestro/templates";

/// Renders the named canonical command for the given provider.
///
/// Reads `.maestro/templates/commands/{command}.md` plus the sibling
/// `manifest.toml`, expands every placeholder via the provider's
/// `template_rules()`, and returns the rendered string. Fails closed on
/// unknown placeholders, missing files, sandbox escapes, and include cycles.
pub fn render_for_provider(
    provider: &dyn AgentProvider,
    command: &str,
) -> Result<String, TemplateError> {
    render_command_in(Path::new(DEFAULT_TEMPLATES_ROOT), provider, command)
}

/// Variant of [`render_for_provider`] that resolves files relative to an
/// explicit templates root — used by integration tests with a tempdir.
pub(crate) fn render_command_in(
    root: &Path,
    provider: &dyn AgentProvider,
    command: &str,
) -> Result<String, TemplateError> {
    render_command_for_rules(root, provider.template_rules(), command)
}

/// Render a canonical command using the supplied provider rules directly.
///
/// Used by `maestro sync-templates`, which iterates `&'static dyn
/// TemplateProviderRules` pointers (one per registered provider) and never
/// needs a full `AgentProvider` instance.
pub fn render_command_for_rules(
    root: &Path,
    rules: &dyn TemplateProviderRules,
    command: &str,
) -> Result<String, TemplateError> {
    validate_command_name(command, root)?;
    let _manifest = Manifest::load(&root.join("manifest.toml"))?;
    let command_path = root.join("commands").join(format!("{command}.md"));
    let input = std::fs::read_to_string(&command_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => TemplateError::FileMissing {
            path: PathBuf::from(&command_path),
        },
        _ => TemplateError::Io {
            path: PathBuf::from(&command_path),
            source: e,
        },
    })?;
    renderer::render_with_source(&input, rules, command_path.to_string_lossy().as_ref(), 0)
}

fn validate_command_name(command: &str, root: &Path) -> Result<(), TemplateError> {
    let reject = || TemplateError::SandboxEscape {
        path: command.to_string(),
        root: root.join("commands").to_string_lossy().into_owned(),
    };
    crate::util::validation::validate_slug(command).map_err(|_| reject())?;
    if crate::util::validation::is_windows_reserved_stem(command) {
        return Err(reject());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn render_str(
    input: &str,
    rules: &dyn TemplateProviderRules,
) -> Result<String, TemplateError> {
    renderer::render(input, rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::agent_provider::types::{
        AgentError, AgentHealthCheck, AgentOutputFormat, AgentProviderEvent, AgentProviderId,
        AgentProviderKind, AgentRequest, AgentRunResult, ParserBinding,
    };

    struct StubProvider;

    #[async_trait]
    impl AgentProvider for StubProvider {
        fn id(&self) -> &str {
            "stub"
        }
        fn kind(&self) -> AgentProviderKind {
            AgentProviderKind::Http
        }
        fn parser_binding(&self) -> ParserBinding {
            ParserBinding {
                name: "stub".to_string(),
                output_format: AgentOutputFormat::StreamJson,
            }
        }
        async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
            Ok(AgentHealthCheck {
                provider_id: AgentProviderId::new(self.id()),
                available: true,
                version: None,
                message: "ok".to_string(),
            })
        }
        async fn run(
            &self,
            _request: AgentRequest,
            _events: mpsc::UnboundedSender<AgentProviderEvent>,
            _cancel: CancellationToken,
        ) -> Result<AgentRunResult, AgentError> {
            Ok(AgentRunResult { exit_code: None })
        }
    }

    #[test]
    fn render_str_with_fake_rules_renders_plain_text() {
        struct YesRules;
        impl TemplateProviderRules for YesRules {
            fn target_dir(&self) -> Option<&'static std::path::Path> {
                None
            }
            fn invoke_subagent(&self, _: &str, _: &str) -> Result<String, TemplateError> {
                Ok(String::new())
            }
            fn hook_gate(&self, _: &str, _: &str) -> Result<String, TemplateError> {
                Ok(String::new())
            }
            fn include(&self, _: &std::path::Path) -> Result<String, TemplateError> {
                Ok(String::new())
            }
            fn subagent_list(&self) -> Result<String, TemplateError> {
                Ok("LIST".to_string())
            }
            fn skill_link(&self, _: &str) -> Result<String, TemplateError> {
                Ok(String::new())
            }
        }
        let out = render_str("plain {{SUBAGENT_LIST}}", &YesRules).expect("ok");
        assert_eq!(out, "plain LIST");
    }

    #[tokio::test]
    async fn default_provider_template_rules_fails_closed_on_render() {
        let provider: Arc<dyn AgentProvider> = Arc::new(StubProvider);
        let result = render_str("{{SUBAGENT_LIST}}", provider.template_rules());
        assert!(matches!(
            result,
            Err(TemplateError::UnsupportedByProvider { ref name, .. }) if name == "SUBAGENT_LIST"
        ));
    }
}
