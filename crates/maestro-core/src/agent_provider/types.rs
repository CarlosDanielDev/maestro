use std::collections::HashMap;
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
    /// Resume an existing provider conversation (#751). Headless claude maps
    /// this to `--resume <id>`; the interactive transport reuses (or
    /// re-attaches to) the PTY child bound to this id.
    pub resume_session_id: Option<String>,
    /// Bypass per-provider pre-spawn gates (e.g. MiniMax quota refusal at
    /// 95%). Defaults to false; CLI flag `--force-quota` flips it on. The
    /// gate still records the spawn and logs a warning at higher levels.
    pub force: bool,
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
            resume_session_id: None,
            force: false,
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
            resume_session_id: None,
            force: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunStarted {
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentRunResult {
    pub exit_code: Option<i32>,
    /// Provider session id for resumable conversations (#751). Headless
    /// claude captures it from the stream (`system`/init or `result` line);
    /// the interactive transport returns the id its PTY child is bound to.
    /// `None` for providers without resume semantics.
    pub session_id: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProviderDefinition {
    pub id: String,
    pub provider: String,
    #[serde(default)]
    pub binary: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Ollama-only context window in tokens. Drives the context-fill gauge.
    #[serde(default)]
    pub num_ctx: Option<u32>,
    /// Claude-only transport selector (#750): `"headless"` (default) or
    /// `"interactive"` (PTY, preserves subscription billing post 2026-06-15).
    #[serde(default)]
    pub transport: Option<String>,
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

    /// Returns the rendering rules used when expanding canonical command
    /// templates for this provider. Default impl returns the fail-closed
    /// [`crate::templates::NullRules`] stub; concrete providers override
    /// once their per-provider rule module lands (issues #703–#705).
    fn template_rules(&self) -> &'static dyn crate::templates::TemplateProviderRules {
        crate::templates::null_rules()
    }
}

#[derive(Clone)]
pub struct AgentProviderFactory {
    default_provider: Arc<dyn AgentProvider>,
    /// Per-agent-id providers for L1 role routing (#897). Empty for the
    /// single-provider default path; populated via [`Self::with_agent_providers`].
    /// Mirrors `SessionPool::agent_providers`.
    agent_providers: HashMap<String, Arc<dyn AgentProvider>>,
}

impl AgentProviderFactory {
    pub fn claude_default() -> Self {
        Self {
            default_provider: Arc::new(crate::agent_provider::claude::ClaudeProvider::default()),
            agent_providers: HashMap::new(),
        }
    }

    pub fn from_config(config: AgentProvidersConfig) -> Result<Self, AgentError> {
        let definition = config
            .providers
            .iter()
            .find(|provider| provider.id == config.default_provider);

        let default_provider: Arc<dyn AgentProvider> = match definition {
            Some(provider) if provider.provider == "claude" || provider.id == "claude" => {
                let binary = provider.binary.as_deref().unwrap_or("claude");
                let transport = crate::agent_provider::claude::ClaudeTransport::from_config_value(
                    provider.transport.as_deref(),
                )?;
                Arc::new(
                    crate::agent_provider::claude::ClaudeProvider::with_transport(
                        binary, transport,
                    ),
                )
            }
            Some(provider) if provider.provider == "qwen" || provider.id == "qwen" => {
                let binary = provider.binary.as_deref().unwrap_or("qwen");
                Arc::new(crate::agent_provider::qwen::QwenProvider::new(binary))
            }
            Some(provider) if provider.provider == "codex" || provider.id == "codex" => {
                let binary = provider.binary.as_deref().unwrap_or("codex");
                Arc::new(crate::agent_provider::codex::CodexProvider::new(binary))
            }
            Some(provider) if provider.provider == "opencode" || provider.id == "opencode" => {
                let binary = provider.binary.as_deref().unwrap_or("opencode");
                Arc::new(crate::agent_provider::opencode::OpenCodeProvider::new(
                    binary,
                ))
            }
            Some(provider) if provider.provider == "ollama" || provider.id == "ollama" => {
                let model = provider.model.clone().ok_or_else(|| {
                    AgentError::Config(format!("agent provider `{}` requires model", provider.id))
                })?;
                let base_url = provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                Arc::new(
                    crate::agent_provider::ollama::OllamaProvider::with_num_ctx(
                        provider.id.clone(),
                        base_url,
                        model,
                        provider.request_timeout_secs.unwrap_or(120),
                        provider.api_key_env.clone(),
                        provider.num_ctx,
                    )
                    .map_err(crate::agent_provider::ollama::OllamaError::into_agent_error)?,
                )
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
                Arc::new(
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
                )
            }
            Some(provider) => {
                return Err(AgentError::Config(format!(
                    "unsupported default agent provider `{}`",
                    provider.provider
                )));
            }
            None if config.default_provider == "claude" => {
                return Ok(Self::claude_default());
            }
            None => {
                return Err(AgentError::Config(format!(
                    "default agent provider `{}` is not configured",
                    config.default_provider
                )));
            }
        };
        Ok(Self::with_default_provider(default_provider))
    }

    pub fn with_default_provider(provider: Arc<dyn AgentProvider>) -> Self {
        Self {
            default_provider: provider,
            agent_providers: HashMap::new(),
        }
    }

    /// Register the per-agent-id provider map used for per-role L1 routing
    /// (#897). Builder-style so existing `with_default_provider(...)` call
    /// sites stay valid. Mirrors `SessionPool::set_agent_providers`.
    pub fn with_agent_providers(
        mut self,
        providers: HashMap<String, Arc<dyn AgentProvider>>,
    ) -> Self {
        self.agent_providers = providers;
        self
    }

    pub fn default_provider(&self) -> Arc<dyn AgentProvider> {
        Arc::clone(&self.default_provider)
    }

    /// Resolve a provider by `agent_id`, falling back to the default provider
    /// when the id is empty or unknown. Never panics — an unknown id is a
    /// valid path (issue #897 edge case: a role binding with no matching
    /// registered provider). Mirrors `SessionPool::resolve_provider`.
    pub fn provider_for_agent_id(&self, agent_id: &str) -> Arc<dyn AgentProvider> {
        if agent_id.is_empty() {
            return self.default_provider();
        }
        self.agent_providers
            .get(agent_id)
            .map(Arc::clone)
            .unwrap_or_else(|| self.default_provider())
    }
}

impl Default for AgentProviderFactory {
    fn default() -> Self {
        Self::claude_default()
    }
}
