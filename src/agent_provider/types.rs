use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::session::types::StreamEvent;

/// Transport class for an agent provider.
///
/// This intentionally describes how a provider is contacted, not how Maestro
/// manages its lifecycle. Subprocess providers and HTTP providers implement the
/// same `run` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProviderKind {
    Subprocess,
    Http,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentProviderId(String);

impl AgentProviderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentOutputFormat {
    StreamJson,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserBinding {
    pub name: String,
    pub output_format: AgentOutputFormat,
}

impl ParserBinding {
    pub fn claude_stream_json() -> Self {
        Self {
            name: "claude-stream-json".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentRequest {
    pub prompt: String,
    pub model: String,
    pub cwd: Option<PathBuf>,
    pub images: Vec<PathBuf>,
    pub output_format: AgentOutputFormat,
    pub permission_mode: Option<String>,
    pub allowed_tools: Vec<String>,
    pub system_prompt_appendix: Option<String>,
}

impl AgentRequest {
    pub fn stream_json(prompt: String, model: String) -> Self {
        Self {
            prompt,
            model,
            cwd: None,
            images: Vec::new(),
            output_format: AgentOutputFormat::StreamJson,
            permission_mode: None,
            allowed_tools: Vec::new(),
            system_prompt_appendix: None,
        }
    }

    pub fn text(prompt: impl Into<String>, model: impl Into<String>, cwd: Option<PathBuf>) -> Self {
        Self {
            prompt: prompt.into(),
            model: model.into(),
            cwd,
            images: Vec::new(),
            output_format: AgentOutputFormat::Text,
            permission_mode: None,
            allowed_tools: Vec::new(),
            system_prompt_appendix: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunStarted {
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunResult {
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentTextOutput {
    pub stdout: String,
    pub stderr: String,
    pub status_success: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentHealthCheck {
    pub provider_id: AgentProviderId,
    pub available: bool,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProviderDefinition {
    pub id: String,
    pub provider: String,
    pub binary: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub request_timeout_secs: Option<u64>,
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProvidersConfig {
    pub default_provider: String,
    pub providers: Vec<AgentProviderDefinition>,
}

impl Default for AgentProvidersConfig {
    fn default() -> Self {
        Self {
            default_provider: "claude".to_string(),
            providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentProviderEvent {
    Started(AgentRunStarted),
    Stream(StreamEvent),
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("failed to spawn {provider_id}: {source}")]
    Spawn {
        provider_id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{provider_id} exited with status {status}: {stderr}")]
    FailedStatus {
        provider_id: String,
        status: String,
        stderr: String,
    },
    #[error("agent stream failed: {0}")]
    Stream(String),
    #[error("{provider_id} run was cancelled")]
    Cancelled { provider_id: String },
    #[error("invalid agent provider config: {0}")]
    Config(String),
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> AgentProviderKind;
    fn parser_binding(&self) -> ParserBinding;

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError>;

    async fn run(
        &self,
        request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError>;
}

#[derive(Clone)]
pub struct AgentProviderFactory {
    default_provider: Arc<dyn AgentProvider>,
}

impl AgentProviderFactory {
    pub fn claude_default() -> Self {
        Self {
            default_provider: Arc::new(crate::agent_provider::claude::ClaudeProvider::default()),
        }
    }

    pub fn from_config(config: AgentProvidersConfig) -> Result<Self, AgentError> {
        let definition = config
            .providers
            .iter()
            .find(|provider| provider.id == config.default_provider);

        match definition {
            Some(provider) if provider.provider == "claude" || provider.id == "claude" => {
                let binary = provider.binary.as_deref().unwrap_or("claude");
                Ok(Self {
                    default_provider: Arc::new(crate::agent_provider::claude::ClaudeProvider::new(
                        binary,
                    )),
                })
            }
            Some(provider) if provider.provider == "qwen" || provider.id == "qwen" => {
                let binary = provider.binary.as_deref().unwrap_or("qwen");
                Ok(Self {
                    default_provider: Arc::new(crate::agent_provider::qwen::QwenProvider::new(
                        binary,
                    )),
                })
            }
            Some(provider) if provider.provider == "codex" || provider.id == "codex" => {
                let binary = provider.binary.as_deref().unwrap_or("codex");
                Ok(Self {
                    default_provider: Arc::new(crate::agent_provider::codex::CodexProvider::new(
                        binary,
                    )),
                })
            }
            Some(provider) if provider.provider == "ollama" || provider.id == "ollama" => {
                let model = provider.model.clone().ok_or_else(|| {
                    AgentError::Config(format!("agent provider `{}` requires model", provider.id))
                })?;
                let base_url = provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                Ok(Self {
                    default_provider: Arc::new(
                        crate::agent_provider::ollama::OllamaProvider::new(
                            provider.id.clone(),
                            base_url,
                            model,
                            provider.request_timeout_secs.unwrap_or(120),
                            provider.api_key_env.clone(),
                        )
                        .map_err(crate::agent_provider::ollama::OllamaError::into_agent_error)?,
                    ),
                })
            }
            Some(provider) if provider.provider == "minimax" || provider.id == "minimax" => {
                let model = provider
                    .model
                    .clone()
                    .unwrap_or_else(|| "MiniMax-M2.7".to_string());
                let base_url = provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.minimax.io/v1".to_string());
                Ok(Self {
                    default_provider: Arc::new(
                        crate::agent_provider::minimax::MinimaxProvider::new(
                            provider.id.clone(),
                            base_url,
                            model,
                            provider.request_timeout_secs.unwrap_or(120),
                            provider
                                .api_key_env
                                .clone()
                                .or_else(|| Some("MINIMAX_API_KEY".to_string())),
                        )
                        .map_err(crate::agent_provider::minimax::MinimaxError::into_agent_error)?,
                    ),
                })
            }
            Some(provider) => Err(AgentError::Config(format!(
                "unsupported default agent provider `{}`",
                provider.provider
            ))),
            None if config.default_provider == "claude" => Ok(Self::claude_default()),
            None => Err(AgentError::Config(format!(
                "default agent provider `{}` is not configured",
                config.default_provider
            ))),
        }
    }

    pub fn with_default_provider(provider: Arc<dyn AgentProvider>) -> Self {
        Self {
            default_provider: provider,
        }
    }

    pub fn default_provider(&self) -> Arc<dyn AgentProvider> {
        Arc::clone(&self.default_provider)
    }
}

impl Default for AgentProviderFactory {
    fn default() -> Self {
        Self::claude_default()
    }
}
