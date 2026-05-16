//! Shared fake `AgentProvider` implementations for tests.
//!
//! Two stub providers used across unit tests (in `src/session/pool.rs`) and
//! integration tests (in `src/integration_tests/templates_runtime.rs`) to
//! exercise the HTTP-provider rendered-template injection path introduced
//! in issue #707. Both implement `AgentProvider::run` as a no-op so the
//! pool can promote sessions against them without spawning processes.

#![cfg(test)]
#![allow(dead_code)]

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::AgentProvider;
use crate::agent_provider::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProviderEvent, AgentProviderId,
    AgentProviderKind, AgentRequest, AgentRunResult, ParserBinding,
};
use crate::templates::TemplateProviderRules;
use crate::templates::provider_rules::{claude_rules, http_generic_rules};

/// HTTP-generic provider stub. `template_rules().target_dir()` returns
/// `None`, so the runtime-injection gate fires for sessions configured
/// against this provider.
pub(crate) struct FakeHttpProvider;

#[async_trait]
impl AgentProvider for FakeHttpProvider {
    fn id(&self) -> &str {
        "qwen"
    }
    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Http
    }
    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "fake-http".to_string(),
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
    fn template_rules(&self) -> &'static dyn TemplateProviderRules {
        http_generic_rules()
    }
}

/// Claude-like provider stub. `template_rules().target_dir()` returns
/// `Some(...)`, so the runtime-injection gate is skipped — Claude
/// discovers rendered templates on disk.
pub(crate) struct FakeClaudeProvider;

#[async_trait]
impl AgentProvider for FakeClaudeProvider {
    fn id(&self) -> &str {
        "claude"
    }
    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }
    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "fake-claude".to_string(),
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
    fn template_rules(&self) -> &'static dyn TemplateProviderRules {
        claude_rules()
    }
}
