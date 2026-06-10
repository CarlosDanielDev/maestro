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
mod interactive;
mod pty_backend;
mod transcript_parser;

use std::sync::Arc;

use std::path::Path;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderKind, AgentRequest, AgentRunResult, AgentTextOutput, ParserBinding,
};

/// Selects how a [`ClaudeProvider`] talks to the `claude` CLI.
///
/// `Headless` is the one-shot `claude --print` subprocess path used today.
/// `Interactive` drives the REPL on a PTY and tails the session transcript
/// (issue #749) so Pro/Max subscription billing survives the 2026-06-15
/// headless cutoff. One-shot text turns stay headless under both variants.
#[derive(Debug, Clone)]
pub enum ClaudeTransport {
    Headless,
    Interactive(InteractiveDriver),
}

impl ClaudeTransport {
    /// Parse the `transport` value from maestro.toml (#750).
    ///
    /// `None`, empty/whitespace (TUI text inputs round-trip cleared fields as
    /// `Some("")`), and `"headless"` all preserve today's behaviour. Anything
    /// other than `"interactive"` is a config error listing the valid values.
    pub fn from_config_value(value: Option<&str>) -> Result<Self, AgentError> {
        match value.map(str::trim) {
            None | Some("") | Some("headless") => Ok(Self::Headless),
            Some("interactive") => Ok(Self::Interactive(InteractiveDriver::PortablePty)),
            Some(other) => Err(AgentError::Config(format!(
                "unknown transport `{other}`, expected one of: headless, interactive"
            ))),
        }
    }
}

/// Driver for the interactive (PTY) transport (issue #749).
///
/// `PortablePty` spawns the Claude REPL on a pseudo-terminal via the
/// `portable-pty` crate and tails the session transcript for events (spike
/// #747). A tmux-based driver may join later behind the `claude-tmux` cargo
/// feature; today that feature is an empty stub.
#[derive(Debug, Clone)]
pub enum InteractiveDriver {
    PortablePty,
}

#[derive(Clone)]
pub struct ClaudeProvider {
    binary: String,
    transport: ClaudeTransport,
    /// Interactive PTY children parked between turns, keyed by session id
    /// (#751). Shared across clones of this provider so a TUI turn task and
    /// the pool see the same children. Empty under the headless transport.
    interactive_slots: interactive::SessionSlots,
}

impl std::fmt::Debug for ClaudeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeProvider")
            .field("binary", &self.binary)
            .field("transport", &self.transport)
            .finish_non_exhaustive()
    }
}

impl ClaudeProvider {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            transport: ClaudeTransport::Headless,
            interactive_slots: interactive::SessionSlots::default(),
        }
    }

    /// Construct with an explicit transport (#750). `new` keeps the headless
    /// default so existing call sites stay unchanged.
    pub fn with_transport(binary: impl Into<String>, transport: ClaudeTransport) -> Self {
        Self {
            binary: binary.into(),
            transport,
            interactive_slots: interactive::SessionSlots::default(),
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
            ClaudeTransport::Interactive(InteractiveDriver::PortablePty) => {
                // Hybrid strategy (spike #747): one-shot text turns stay on
                // the headless path; only stream-json turns (the long-lived
                // interaction surface) ride the PTY.
                if matches!(request.output_format, AgentOutputFormat::Text) {
                    return headless::run(&self.binary, self.id(), request, events, cancel).await;
                }
                let home = std::env::var("HOME").map_err(|_| {
                    AgentError::Config(
                        "interactive transport requires HOME to locate the transcript".to_string(),
                    )
                })?;
                let spec = interactive::RunSpec::new(
                    self.binary.clone(),
                    self.id(),
                    std::path::PathBuf::from(home),
                    uuid::Uuid::new_v4().to_string(),
                );
                interactive::run_session_turn(
                    Arc::new(pty_backend::PortablePtyBackend),
                    Arc::clone(&self.interactive_slots),
                    spec,
                    request,
                    events,
                    cancel,
                )
                .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_from_config_value_maps_all_cases() {
        assert!(matches!(
            ClaudeTransport::from_config_value(None),
            Ok(ClaudeTransport::Headless)
        ));
        assert!(matches!(
            ClaudeTransport::from_config_value(Some("")),
            Ok(ClaudeTransport::Headless)
        ));
        assert!(matches!(
            ClaudeTransport::from_config_value(Some("headless")),
            Ok(ClaudeTransport::Headless)
        ));
        assert!(matches!(
            ClaudeTransport::from_config_value(Some("interactive")),
            Ok(ClaudeTransport::Interactive(InteractiveDriver::PortablePty))
        ));
        match ClaudeTransport::from_config_value(Some("bogus")) {
            Err(AgentError::Config(msg)) => {
                assert!(msg.contains("headless, interactive"), "{msg}");
                assert!(msg.contains("bogus"), "{msg}");
            }
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn interactive_transport_with_missing_binary_fails_spawn() {
        let provider = ClaudeProvider::with_transport(
            "/nonexistent-sentinel-binary-maestro-test",
            ClaudeTransport::Interactive(InteractiveDriver::PortablePty),
        );

        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let request = AgentRequest::stream_json("hello".into(), "claude-sonnet".into());

        let result = provider.run(request, tx, cancel).await;

        assert!(
            matches!(result, Err(AgentError::Spawn { .. })),
            "expected Spawn error for a missing binary, got: {result:?}"
        );
        assert!(
            rx.try_recv().is_err(),
            "no provider events may be emitted before a successful spawn"
        );
    }

    #[tokio::test]
    async fn interactive_transport_routes_text_format_through_headless() {
        // Hybrid strategy: one-shot text turns must NOT ride the PTY. The
        // sentinel binary makes the headless path fail with Spawn — reaching
        // it (instead of openpty + transcript polling) is the assertion.
        let provider = ClaudeProvider::with_transport(
            "/nonexistent-sentinel-binary-maestro-test",
            ClaudeTransport::Interactive(InteractiveDriver::PortablePty),
        );

        let (tx, _rx) = mpsc::unbounded_channel();
        let request = AgentRequest::text("one shot", "claude-sonnet", None);

        let result = provider.run(request, tx, CancellationToken::new()).await;

        assert!(
            matches!(result, Err(AgentError::Spawn { .. })),
            "text format must hit the headless spawn path: {result:?}"
        );
    }
}
