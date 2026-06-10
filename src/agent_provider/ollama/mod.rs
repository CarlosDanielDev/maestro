#![deny(clippy::unwrap_used)]

use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod http;
pub mod types;

pub use http::OllamaHttpClient;
pub use types::OllamaError;

use crate::session::types::StreamEvent;

use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    ParserBinding,
};

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    id: String,
    model: String,
    http: OllamaHttpClient,
    num_ctx: Option<u32>,
}

impl OllamaProvider {
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
    ) -> Result<Self, OllamaError> {
        Self::with_num_ctx(id, base_url, model, request_timeout_secs, api_key_env, None)
    }

    pub fn with_num_ctx(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
        num_ctx: Option<u32>,
    ) -> Result<Self, OllamaError> {
        let timeout = Duration::from_secs(request_timeout_secs);
        Ok(Self {
            id: id.into(),
            model: model.into(),
            http: OllamaHttpClient::new(base_url, timeout, api_key_env)?,
            num_ctx,
        })
    }
}

#[async_trait::async_trait]
impl AgentProvider for OllamaProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Http
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "openai-compatible-sse".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        let version = self
            .http
            .version()
            .await
            .map_err(OllamaError::into_agent_error)?;
        let model_available = self
            .http
            .model_available(&self.model)
            .await
            .map_err(OllamaError::into_agent_error)?;

        if !model_available {
            return Err(OllamaError::ModelNotPulled {
                model: self.model.clone(),
            }
            .into_agent_error());
        }

        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: Some(version.clone()),
            message: format!("Ollama {version}; model `{}` available", self.model),
        })
    }

    fn template_rules(&self) -> &'static dyn crate::templates::TemplateProviderRules {
        crate::templates::provider_rules::http_generic_rules()
    }

    async fn run(
        &self,
        request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        let model = if request.model.trim().is_empty() {
            self.model.as_str()
        } else {
            request.model.as_str()
        };

        let mut stream = self
            .http
            .chat_stream(model, &request.prompt)
            .await
            .map_err(OllamaError::into_agent_error)?;
        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
            process_id: None,
        }));

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(AgentError::Cancelled {
                        provider_id: self.id().to_string(),
                    });
                }
                next = stream.recv() => {
                    let Some(next) = next else {
                        break;
                    };
                    match next {
                        Ok(event) => {
                            // Derive a ContextUpdate from the shared SSE
                            // parser's TokenUpdate when num_ctx is configured.
                            // The SSE parser is provider-agnostic; only Ollama
                            // surfaces context fill, so the transform lives in
                            // the provider rather than the parser. Cap at 1.0
                            // so a buggy backend returning > num_ctx tokens
                            // doesn't surface as a percentage above 100%.
                            if let StreamEvent::TokenUpdate { ref usage } = event
                                && let Some(num_ctx) = self.num_ctx
                                && num_ctx > 0
                            {
                                let pct = (usage.input_tokens as f64) / f64::from(num_ctx);
                                let _ = events.send(AgentProviderEvent::Stream(
                                    StreamEvent::ContextUpdate { context_pct: pct.min(1.0) },
                                ));
                            }
                            let _ = events.send(AgentProviderEvent::Stream(event));
                        }
                        Err(err) => {
                            return Err(err.into_agent_error());
                        }
                    }
                }
            }
        }

        Ok(AgentRunResult {
            exit_code: None,
            session_id: None,
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
