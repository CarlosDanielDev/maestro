#![deny(clippy::unwrap_used)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OllamaError {
    #[error(
        "could not connect to Ollama at {base_url}; run 'ollama serve' to start the local server"
    )]
    ConnectionRefused { base_url: String },
    #[error("Ollama returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("malformed Ollama SSE stream: {0}")]
    MalformedSse(String),
    #[error("Ollama model `{model}` is not available; run 'ollama pull {model}' to download it")]
    ModelNotPulled { model: String },
    #[error("Ollama request timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("Ollama request failed: {0}")]
    Request(String),
}

impl OllamaError {
    pub fn into_agent_error(self) -> crate::agent_provider::types::AgentError {
        crate::agent_provider::types::AgentError::Stream(self.to_string())
    }
}
