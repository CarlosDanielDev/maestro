#![deny(clippy::unwrap_used)]

use std::time::Duration;

use futures::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use super::types::OllamaError;
use crate::agent_provider::openai_compat::sse::OpenAiCompatibleSseParser;
use crate::session::types::StreamEvent;

#[derive(Debug, Clone)]
pub struct OllamaHttpClient {
    client: reqwest::Client,
    base_url: String,
    timeout: Duration,
    api_key_env: Option<String>,
}

impl OllamaHttpClient {
    pub fn new(
        base_url: impl Into<String>,
        timeout: Duration,
        api_key_env: Option<String>,
    ) -> Result<Self, OllamaError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| OllamaError::Request(err.to_string()))?;

        Ok(Self {
            client,
            base_url: trim_base_url(base_url.into()),
            timeout,
            api_key_env,
        })
    }

    pub async fn version(&self) -> Result<String, OllamaError> {
        let response = self
            .request_with_retry(self.client.get(self.url("/api/version")))
            .await?;
        if !response.status().is_success() {
            return Err(OllamaError::HttpStatus(response.status().as_u16()));
        }
        let version: VersionResponse = response
            .json()
            .await
            .map_err(|err| OllamaError::Request(err.to_string()))?;
        Ok(version.version)
    }

    pub async fn model_available(&self, model: &str) -> Result<bool, OllamaError> {
        let response = self
            .request_with_retry(self.client.get(self.url("/api/tags")))
            .await?;
        if !response.status().is_success() {
            return Err(OllamaError::HttpStatus(response.status().as_u16()));
        }
        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|err| OllamaError::Request(err.to_string()))?;
        Ok(tags.models.iter().any(|entry| entry.name == model))
    }

    pub async fn chat_stream(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<mpsc::Receiver<Result<StreamEvent, OllamaError>>, OllamaError> {
        let body = json!({
            "model": model,
            "stream": true,
            "messages": [
                { "role": "user", "content": prompt }
            ]
        });
        let mut builder = self
            .client
            .post(self.url("/v1/chat/completions"))
            .json(&body);
        if let Some(token) = self.api_key() {
            builder = builder.bearer_auth(token);
        }

        let response = self.request_with_retry(builder).await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_status(status, &body, model));
        }

        let (tx, rx) = mpsc::channel(64);
        let timeout_secs = self.timeout.as_secs();
        tokio::spawn(async move {
            let mut parser = OpenAiCompatibleSseParser::new();
            let mut stream = response.bytes_stream();

            while let Some(next) = stream.next().await {
                match next {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        match parser.push_chunk(&text) {
                            Ok(events) => {
                                for event in events {
                                    if tx.send(Ok(event)).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(Err(OllamaError::MalformedSse(err))).await;
                                return;
                            }
                        }
                    }
                    Err(err) if err.is_timeout() => {
                        let _ = tx
                            .send(Err(OllamaError::Timeout {
                                seconds: timeout_secs,
                            }))
                            .await;
                        return;
                    }
                    Err(err) => {
                        let _ = tx.send(Err(OllamaError::Request(err.to_string()))).await;
                        return;
                    }
                }
            }

            match parser.finish() {
                Ok(events) => {
                    for event in events {
                        if tx.send(Ok(event)).await.is_err() {
                            return;
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(OllamaError::MalformedSse(err))).await;
                }
            }
        });

        Ok(rx)
    }

    async fn request_with_retry(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, OllamaError> {
        let retry = request.try_clone();
        match self.send(request).await {
            Err(err @ OllamaError::ConnectionRefused { .. }) => {
                if let Some(retry) = retry {
                    tracing::warn!("Ollama connection failed; retrying once");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    self.send(retry).await
                } else {
                    Err(err)
                }
            }
            other => other,
        }
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, OllamaError> {
        request.send().await.map_err(|err| {
            if err.is_timeout() {
                OllamaError::Timeout {
                    seconds: self.timeout.as_secs(),
                }
            } else if err.is_connect() {
                OllamaError::ConnectionRefused {
                    base_url: self.base_url.clone(),
                }
            } else {
                OllamaError::Request(err.to_string())
            }
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn api_key(&self) -> Option<String> {
        self.api_key_env
            .as_deref()
            .and_then(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    version: String,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    name: String,
}

fn trim_base_url(mut base_url: String) -> String {
    while base_url.ends_with('/') {
        base_url.pop();
    }
    base_url
}

fn map_status(status: StatusCode, body: &str, model: &str) -> OllamaError {
    let lower = body.to_ascii_lowercase();
    if status == StatusCode::NOT_FOUND
        || lower.contains("model not found")
        || lower.contains("not found")
    {
        OllamaError::ModelNotPulled {
            model: model.to_string(),
        }
    } else {
        OllamaError::HttpStatus(status.as_u16())
    }
}
