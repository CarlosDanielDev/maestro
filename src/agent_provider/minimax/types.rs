#![deny(clippy::unwrap_used)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MinimaxError {
    #[error("MiniMax API key is missing; set {env} to your MiniMax API key")]
    MissingApiKey { env: String },
    #[error("invalid {env} — check your key at platform.minimax.io")]
    Unauthorized { env: String },
    #[error("MiniMax returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("malformed MiniMax SSE stream: {0}")]
    MalformedSse(String),
    #[error("MiniMax request timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("MiniMax network issue: {0}")]
    Network(String),
    #[error("MiniMax request failed: {0}")]
    Request(String),
}

impl MinimaxError {
    pub fn into_agent_error(self) -> crate::agent_provider::types::AgentError {
        crate::agent_provider::types::AgentError::Stream(self.to_string())
    }
}
