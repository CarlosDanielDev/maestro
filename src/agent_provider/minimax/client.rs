#![deny(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use reqwest::StatusCode;
use serde_json::json;
use tokio::sync::mpsc;

use super::types::MinimaxError;
use crate::agent_provider::openai_compat::sse::OpenAiCompatibleSseParser;
use crate::config::agents_remote_auth as policy;
use crate::session::types::StreamEvent;

type ApiKeyLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Clone)]
pub struct MinimaxClient {
    client: reqwest::Client,
    base_url: String,
    timeout: Duration,
    api_key_env: String,
    api_key_lookup: ApiKeyLookup,
    allow_remote_auth: bool,
}

impl std::fmt::Debug for MinimaxClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinimaxClient")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("api_key_env", &self.api_key_env)
            .finish_non_exhaustive()
    }
}

impl MinimaxClient {
    pub fn new(
        base_url: impl Into<String>,
        timeout: Duration,
        api_key_env: impl Into<String>,
    ) -> Result<Self, MinimaxError> {
        Self::with_api_key_lookup(base_url, timeout, api_key_env, |name| {
            std::env::var(name).ok()
        })
    }

    pub(crate) fn with_api_key_lookup(
        base_url: impl Into<String>,
        timeout: Duration,
        api_key_env: impl Into<String>,
        api_key_lookup: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Result<Self, MinimaxError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| MinimaxError::Request(err.to_string()))?;

        Ok(Self {
            client,
            base_url: trim_base_url(base_url.into()),
            timeout,
            api_key_env: api_key_env.into(),
            api_key_lookup: Arc::new(api_key_lookup),
            allow_remote_auth: false,
        })
    }

    /// Confirm the configured `base_url` host for bearer auth (#1030).
    /// Without this the client attaches credentials only for the MiniMax
    /// default host or loopback.
    pub(crate) fn with_allow_remote_auth(mut self, allow: bool) -> Self {
        self.allow_remote_auth = allow;
        self
    }

    /// Request-layer re-check of the config policy (defense in depth).
    pub(crate) fn remote_auth_allowed(&self) -> bool {
        policy::credentials_allowed(
            &self.base_url,
            policy::MINIMAX_DEFAULT_BASE_URL,
            self.allow_remote_auth,
        )
    }

    /// Refuse credentialed requests to hosts outside {provider default,
    /// loopback} unless `allow_remote_auth` confirmed them (#1030).
    fn ensure_credentials_allowed(&self) -> Result<(), MinimaxError> {
        if self.remote_auth_allowed() {
            return Ok(());
        }
        let host = policy::split_base_url(&self.base_url)
            .map(|(_, host)| host)
            .unwrap_or_else(|| self.base_url.clone());
        Err(MinimaxError::RemoteAuthRefused { host })
    }

    pub async fn models_health(&self) -> Result<(), MinimaxError> {
        self.ensure_credentials_allowed()?;
        let token = self.api_key()?;
        tracing::debug!(
            provider = "minimax",
            api_key_env = %self.api_key_env,
            "checking MiniMax models endpoint"
        );
        let response = self
            .send(self.client.get(self.url("/models")).bearer_auth(token))
            .await?;
        map_empty_status(response.status(), &self.api_key_env)
    }

    pub async fn chat_stream(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<mpsc::Receiver<Result<StreamEvent, MinimaxError>>, MinimaxError> {
        self.ensure_credentials_allowed()?;
        let token = self.api_key()?;
        let body = json!({
            "model": model,
            "stream": true,
            "messages": [
                { "role": "user", "content": prompt }
            ]
        });
        tracing::debug!(
            provider = "minimax",
            api_key_env = %self.api_key_env,
            model = %model,
            "starting MiniMax chat completion stream"
        );
        let response = self
            .send(
                self.client
                    .post(self.url("/chat/completions"))
                    .bearer_auth(token)
                    .json(&body),
            )
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_status(status, &self.api_key_env));
        }

        let (tx, rx) = mpsc::channel(64);
        let timeout_secs = self.timeout.as_secs();
        tokio::spawn(async move {
            let mut parser = OpenAiCompatibleSseParser::new();
            let mut stream = response.bytes_stream();

            while let Some(next) = stream.next().await {
                match next {
                    Ok(bytes) => match parser.push_bytes(&bytes) {
                        Ok(events) => {
                            for event in events {
                                if tx.send(Ok(event)).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(MinimaxError::MalformedSse(err))).await;
                            return;
                        }
                    },
                    Err(err) if err.is_timeout() => {
                        let _ = tx
                            .send(Err(MinimaxError::Timeout {
                                seconds: timeout_secs,
                            }))
                            .await;
                        return;
                    }
                    Err(err) => {
                        let _ = tx.send(Err(MinimaxError::Network(err.to_string()))).await;
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
                    let _ = tx.send(Err(MinimaxError::MalformedSse(err))).await;
                }
            }
        });

        Ok(rx)
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, MinimaxError> {
        request.send().await.map_err(|err| {
            if err.is_timeout() {
                MinimaxError::Timeout {
                    seconds: self.timeout.as_secs(),
                }
            } else if err.is_connect() {
                MinimaxError::Network(format!("could not connect to {}", self.base_url))
            } else {
                MinimaxError::Request(err.to_string())
            }
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn api_key(&self) -> Result<String, MinimaxError> {
        (self.api_key_lookup)(&self.api_key_env)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| MinimaxError::MissingApiKey {
                env: self.api_key_env.clone(),
            })
    }
}

fn trim_base_url(mut base_url: String) -> String {
    while base_url.ends_with('/') {
        base_url.pop();
    }
    base_url
}

fn map_empty_status(status: StatusCode, api_key_env: &str) -> Result<(), MinimaxError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(map_status(status, api_key_env))
    }
}

fn map_status(status: StatusCode, api_key_env: &str) -> MinimaxError {
    if status == StatusCode::UNAUTHORIZED {
        MinimaxError::Unauthorized {
            env: api_key_env.to_string(),
        }
    } else {
        MinimaxError::HttpStatus(status.as_u16())
    }
}
