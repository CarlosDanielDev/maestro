#![deny(clippy::unwrap_used)]

use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod http;
pub mod types;

pub use http::OllamaHttpClient;
pub use types::OllamaError;

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
}

impl OllamaProvider {
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
    ) -> Result<Self, OllamaError> {
        let timeout = Duration::from_secs(request_timeout_secs);
        Ok(Self {
            id: id.into(),
            model: model.into(),
            http: OllamaHttpClient::new(base_url, timeout, api_key_env)?,
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
        let provider =
            OllamaProvider::new("ollama", base_url, "llama3.2", 5, None).expect("provider");
        let (tx, mut rx) = mpsc::unbounded_channel();

        provider
            .run(
                AgentRequest::stream_json("say hi".to_string(), "llama3.2".to_string()),
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
    async fn maps_model_not_pulled_status() {
        let base_url = spawn_test_server(
            "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\n\r\n\
             {\"error\":\"model not found\"}",
        )
        .await;
        let provider =
            OllamaProvider::new("ollama", base_url, "missing-model", 5, None).expect("provider");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = provider
            .run(
                AgentRequest::stream_json("say hi".to_string(), "missing-model".to_string()),
                tx,
                CancellationToken::new(),
            )
            .await
            .expect_err("missing model should fail");

        assert!(err.to_string().contains("run 'ollama pull missing-model'"));
    }

    #[tokio::test]
    async fn health_check_fetches_version_and_verifies_model_tag() {
        let base_url = spawn_test_server_sequence(vec![
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n\
             {\"version\":\"0.5.7\"}",
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n\
             {\"models\":[{\"name\":\"llama3.2\"}]}",
        ])
        .await;
        let provider =
            OllamaProvider::new("ollama", base_url, "llama3.2", 5, None).expect("provider");

        let health = provider.health_check().await.expect("health check");

        assert!(health.available);
        assert_eq!(health.version.as_deref(), Some("0.5.7"));
        assert!(health.message.contains("model `llama3.2` available"));
    }

    #[tokio::test]
    async fn health_check_reports_model_pull_hint_when_tag_is_missing() {
        let base_url = spawn_test_server_sequence(vec![
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n\
             {\"version\":\"0.5.7\"}",
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n\
             {\"models\":[{\"name\":\"other\"}]}",
        ])
        .await;
        let provider =
            OllamaProvider::new("ollama", base_url, "missing-model", 5, None).expect("provider");

        let err = provider.health_check().await.expect_err("missing model");

        assert!(err.to_string().contains("ollama pull missing-model"));
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

    async fn spawn_test_server_sequence(responses: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            for response in responses {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut request = vec![0_u8; 2048];
                    let _ = socket.read(&mut request).await;
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });
        format!("http://{addr}")
    }
}
