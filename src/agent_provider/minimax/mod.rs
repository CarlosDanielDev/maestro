#![deny(clippy::unwrap_used)]

use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod client;
pub mod types;

pub use client::MinimaxClient;
pub use types::MinimaxError;

use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    ParserBinding,
};

const DEFAULT_API_KEY_ENV: &str = "MINIMAX_API_KEY";

#[derive(Debug, Clone)]
pub struct MinimaxProvider {
    id: String,
    model: String,
    http: MinimaxClient,
}

impl MinimaxProvider {
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
    ) -> Result<Self, MinimaxError> {
        Self::with_client(
            id,
            model,
            MinimaxClient::new(
                base_url,
                Duration::from_secs(request_timeout_secs),
                api_key_env.unwrap_or_else(|| DEFAULT_API_KEY_ENV.to_string()),
            )?,
        )
    }

    fn with_client(
        id: impl Into<String>,
        model: impl Into<String>,
        http: MinimaxClient,
    ) -> Result<Self, MinimaxError> {
        Ok(Self {
            id: id.into(),
            model: model.into(),
            http,
        })
    }

    #[cfg(test)]
    fn new_with_api_key_lookup(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
        api_key_lookup: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Result<Self, MinimaxError> {
        Self::with_client(
            id,
            model,
            MinimaxClient::with_api_key_lookup(
                base_url,
                Duration::from_secs(request_timeout_secs),
                api_key_env.unwrap_or_else(|| DEFAULT_API_KEY_ENV.to_string()),
                api_key_lookup,
            )?,
        )
    }
}

#[async_trait::async_trait]
impl AgentProvider for MinimaxProvider {
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
        self.http
            .models_health()
            .await
            .map_err(MinimaxError::into_agent_error)?;

        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: None,
            message: format!(
                "MiniMax models endpoint reachable; model `{}` configured",
                self.model
            ),
        })
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
            .map_err(MinimaxError::into_agent_error)?;
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
                            let _ = events.send(AgentProviderEvent::Stream(event));
                        }
                        Err(err) => {
                            return Err(err.into_agent_error());
                        }
                    }
                }
            }
        }

        Ok(AgentRunResult { exit_code: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::StreamEvent;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn streams_openai_compatible_sse() {
        let base_url = spawn_test_server(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n\
             data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n\
             data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
             data: [DONE]\n\n",
        )
        .await;
        let provider = MinimaxProvider::new_with_api_key_lookup(
            "minimax",
            base_url,
            "MiniMax-M2.7",
            5,
            Some("MINIMAX_API_KEY".to_string()),
            |_| Some("test-key".to_string()),
        )
        .expect("provider");
        let (tx, mut rx) = mpsc::unbounded_channel();

        provider
            .run(
                AgentRequest::stream_json("say hi".to_string(), "MiniMax-M2.7".to_string()),
                tx,
                CancellationToken::new(),
            )
            .await
            .expect("run");

        assert!(matches!(
            rx.recv().await,
            Some(AgentProviderEvent::Started(_))
        ));
        assert!(matches!(
            rx.recv().await,
            Some(AgentProviderEvent::Stream(StreamEvent::AssistantMessage { text })) if text == "hello"
        ));
        assert!(matches!(
            rx.recv().await,
            Some(AgentProviderEvent::Stream(StreamEvent::Completed { .. }))
        ));
    }

    #[tokio::test]
    async fn maps_unauthorized_status() {
        let base_url = spawn_test_server(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\n\r\n\
             {\"error\":\"invalid key\"}",
        )
        .await;
        let provider = MinimaxProvider::new_with_api_key_lookup(
            "minimax",
            base_url,
            "MiniMax-M2.7",
            5,
            Some("MINIMAX_API_KEY".to_string()),
            |_| Some("test-key".to_string()),
        )
        .expect("provider");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = provider
            .run(
                AgentRequest::stream_json("say hi".to_string(), "MiniMax-M2.7".to_string()),
                tx,
                CancellationToken::new(),
            )
            .await
            .expect_err("401 should fail");

        assert!(
            err.to_string()
                .contains("invalid MINIMAX_API_KEY — check your key at platform.minimax.io")
        );
    }

    #[tokio::test]
    async fn missing_api_key_uses_env_var_name_only() {
        let base_url = spawn_test_server(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"data\":[]}",
        )
        .await;
        let provider = MinimaxProvider::new_with_api_key_lookup(
            "minimax",
            base_url,
            "MiniMax-M2.7",
            5,
            Some("MINIMAX_API_KEY".to_string()),
            |_| None,
        )
        .expect("provider");
        let err = provider.health_check().await.expect_err("missing key");
        let rendered = err.to_string();

        assert!(rendered.contains("set MINIMAX_API_KEY to your MiniMax API key"));
        assert!(!rendered.contains("secret-value"));
    }

    #[tokio::test]
    async fn health_check_passes_when_models_endpoint_is_reachable() {
        let base_url = spawn_test_server(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"data\":[]}",
        )
        .await;
        let provider = MinimaxProvider::new_with_api_key_lookup(
            "minimax",
            base_url,
            "MiniMax-M2.7",
            5,
            Some("MINIMAX_API_KEY".to_string()),
            |_| Some("test-key".to_string()),
        )
        .expect("provider");

        let health = provider.health_check().await.expect("health check");

        assert!(health.available);
        assert!(health.message.contains("models endpoint reachable"));
    }

    #[tokio::test]
    async fn health_check_maps_unauthorized_without_exposing_key_value() {
        let base_url = spawn_test_server(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\n\r\n\
             {\"error\":\"invalid key\"}",
        )
        .await;
        let provider = MinimaxProvider::new_with_api_key_lookup(
            "minimax",
            base_url,
            "MiniMax-M2.7",
            5,
            Some("MINIMAX_API_KEY".to_string()),
            |_| Some("secret-value".to_string()),
        )
        .expect("provider");

        let err = provider.health_check().await.expect_err("401");
        let rendered = err.to_string();

        assert!(rendered.contains("invalid MINIMAX_API_KEY"));
        assert!(!rendered.contains("secret-value"));
    }

    async fn spawn_test_server(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut request = vec![0_u8; 2048];
                let _ = socket.read(&mut request).await;
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{addr}")
    }
}
