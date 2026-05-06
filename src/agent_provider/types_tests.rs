#![deny(clippy::unwrap_used)]

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::types::*;
use crate::session::types::StreamEvent;

struct HttpStubProvider;

#[async_trait]
impl AgentProvider for HttpStubProvider {
    fn id(&self) -> &str {
        "http-stub"
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Http
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "stub-json".to_string(),
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
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
            process_id: None,
        }));
        let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
            cost_usd: 0.0,
        }));
        Ok(AgentRunResult { exit_code: None })
    }
}

#[tokio::test]
async fn trait_supports_http_provider_without_subprocess_state() {
    let provider: Arc<dyn AgentProvider> = Arc::new(HttpStubProvider);
    assert_eq!(provider.kind(), AgentProviderKind::Http);

    let (tx, mut rx) = mpsc::unbounded_channel();
    provider
        .run(
            AgentRequest::stream_json("prompt".into(), "model".into()),
            tx,
            CancellationToken::new(),
        )
        .await
        .unwrap();

    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Started(AgentRunStarted {
            process_id: None
        }))
    ));
    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Stream(StreamEvent::Completed { .. }))
    ));
}

#[test]
fn factory_defaults_to_claude_provider() {
    let factory = AgentProviderFactory::default();
    assert_eq!(factory.default_provider().id(), "claude");
}

#[test]
fn factory_accepts_empty_config_as_legacy_claude() {
    let factory = AgentProviderFactory::from_config(AgentProvidersConfig::default()).unwrap();
    assert_eq!(factory.default_provider().id(), "claude");
}

#[test]
fn factory_accepts_qwen_provider() {
    let factory = AgentProviderFactory::from_config(AgentProvidersConfig {
        default_provider: "qwen".to_string(),
        providers: vec![AgentProviderDefinition {
            id: "qwen".to_string(),
            provider: "qwen".to_string(),
            binary: Some("qwen".to_string()),
            base_url: None,
            model: None,
            request_timeout_secs: None,
            api_key_env: None,
        }],
    })
    .unwrap();

    assert_eq!(factory.default_provider().id(), "qwen");
}

#[test]
fn factory_accepts_codex_provider() {
    let factory = AgentProviderFactory::from_config(AgentProvidersConfig {
        default_provider: "codex".to_string(),
        providers: vec![AgentProviderDefinition {
            id: "codex".to_string(),
            provider: "codex".to_string(),
            binary: Some("codex".to_string()),
            base_url: None,
            model: None,
            request_timeout_secs: None,
            api_key_env: None,
        }],
    })
    .unwrap();

    assert_eq!(factory.default_provider().id(), "codex");
}

#[test]
fn factory_accepts_opencode_provider() {
    let factory = AgentProviderFactory::from_config(AgentProvidersConfig {
        default_provider: "opencode".to_string(),
        providers: vec![AgentProviderDefinition {
            id: "opencode".to_string(),
            provider: "opencode".to_string(),
            binary: Some("opencode".to_string()),
            base_url: None,
            model: Some("anthropic/claude-sonnet-4-5".to_string()),
            request_timeout_secs: None,
            api_key_env: None,
        }],
    })
    .unwrap();

    assert_eq!(factory.default_provider().id(), "opencode");
    assert_eq!(
        factory.default_provider().kind(),
        AgentProviderKind::Subprocess
    );
}

#[test]
fn factory_accepts_ollama_provider() {
    let factory = AgentProviderFactory::from_config(AgentProvidersConfig {
        default_provider: "ollama".to_string(),
        providers: vec![AgentProviderDefinition {
            id: "ollama".to_string(),
            provider: "ollama".to_string(),
            binary: None,
            base_url: Some("http://localhost:11434".to_string()),
            model: Some("llama3.2".to_string()),
            request_timeout_secs: Some(5),
            api_key_env: None,
        }],
    })
    .unwrap();

    assert_eq!(factory.default_provider().id(), "ollama");
    assert_eq!(factory.default_provider().kind(), AgentProviderKind::Http);
}

#[test]
fn factory_accepts_minimax_provider() {
    let factory = AgentProviderFactory::from_config(AgentProvidersConfig {
        default_provider: "minimax".to_string(),
        providers: vec![AgentProviderDefinition {
            id: "minimax".to_string(),
            provider: "minimax".to_string(),
            binary: None,
            base_url: Some("https://api.minimax.io/v1".to_string()),
            model: Some("MiniMax-M2.7".to_string()),
            request_timeout_secs: Some(5),
            api_key_env: Some("MINIMAX_API_KEY".to_string()),
        }],
    })
    .unwrap();

    assert_eq!(factory.default_provider().id(), "minimax");
    assert_eq!(factory.default_provider().kind(), AgentProviderKind::Http);
}
