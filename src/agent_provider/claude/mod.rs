//! Claude CLI agent provider.
//!
//! `ClaudeProvider::run` dispatches on a [`ClaudeTransport`] enum so a future
//! interactive (PTY) backend can plug in alongside the existing headless
//! one-shot path without touching the headless logic (issue #748). Today the
//! transport defaults to [`ClaudeTransport::Headless`] and behaviour is
//! unchanged — see RUST-GUARDRAILS.md §1 (module discipline) and §5
//! (subprocess lifecycle preserved). The headless spawn logic lives in
//! `headless.rs`; the inherent methods here are thin shells that supply
//! `binary` + `id()` to those free functions.

mod headless;

use std::path::Path;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::types::{
    AgentError, AgentHealthCheck, AgentProvider, AgentProviderEvent, AgentProviderKind,
    AgentRequest, AgentRunResult, AgentTextOutput, ParserBinding,
};

/// Selects how a [`ClaudeProvider`] talks to the `claude` CLI.
///
/// `Headless` is the one-shot `claude --print` subprocess path used today.
/// `Interactive` is the seam for the upcoming PTY-based transport (issue #749);
/// it is not implemented yet and `run` returns a config error for it.
#[derive(Debug, Clone)]
pub enum ClaudeTransport {
    Headless,
    Interactive(InteractiveDriver),
}

/// Driver for the interactive (PTY) transport.
///
/// The PTY backend lands in issue #749. Until then this carries a single
/// `Unimplemented` marker so the dispatch arm in [`ClaudeProvider::run`] is
/// reachable and testable. (The issue spec wrote this as an empty enum, but an
/// uninhabited type makes both the dispatch arm and its required test
/// impossible — the marker is the minimal inhabited form.)
// TODO(#749): replace `Unimplemented` with the real PTY driver state.
#[derive(Debug, Clone)]
pub enum InteractiveDriver {
    Unimplemented,
}

#[derive(Debug, Clone)]
pub struct ClaudeProvider {
    binary: String,
    transport: ClaudeTransport,
}

impl ClaudeProvider {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            transport: ClaudeTransport::Headless,
        }
    }

    pub fn build_stream_args(&self, request: &AgentRequest) -> Vec<String> {
        headless::build_stream_args(request)
    }

    pub fn build_text_args(&self, request: &AgentRequest) -> Vec<String> {
        headless::build_text_args(request)
    }

    pub async fn run_text(
        &self,
        model: &str,
        prompt: &str,
        cwd: Option<&Path>,
    ) -> Result<AgentTextOutput, AgentError> {
        headless::run_text(&self.binary, self.id(), model, prompt, cwd).await
    }

    pub fn health_check_blocking(&self) -> AgentHealthCheck {
        headless::health_check_blocking(&self.binary, self.id())
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new("claude")
    }
}

#[async_trait::async_trait]
impl AgentProvider for ClaudeProvider {
    fn id(&self) -> &str {
        "claude"
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding::claude_stream_json()
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        headless::health_check(&self.binary, self.id()).await
    }

    fn template_rules(&self) -> &'static dyn crate::templates::TemplateProviderRules {
        crate::templates::provider_rules::claude_rules()
    }

    async fn run(
        &self,
        request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        match self.transport {
            ClaudeTransport::Headless => {
                headless::run(&self.binary, self.id(), request, events, cancel).await
            }
            ClaudeTransport::Interactive(_) => Err(AgentError::Config(
                "interactive transport not implemented (see PTY backend issue)".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interactive_transport_returns_not_implemented_error() {
        let provider = ClaudeProvider {
            binary: "/nonexistent-sentinel-binary-maestro-test".to_string(),
            transport: ClaudeTransport::Interactive(InteractiveDriver::Unimplemented),
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let request = AgentRequest::stream_json("hello".into(), "claude-sonnet".into());

        let result = provider.run(request, tx, cancel).await;

        match result {
            Err(AgentError::Config(msg)) => {
                assert!(
                    msg.contains("interactive transport not implemented"),
                    "unexpected Config message: {msg}"
                );
            }
            Err(AgentError::Spawn { .. }) => {
                panic!("sentinel binary was reached — dispatch did not short-circuit");
            }
            other => panic!("expected Config error, got: {other:?}"),
        }

        assert!(
            rx.try_recv().is_err(),
            "Interactive arm must not emit any provider events before returning"
        );
    }
}
