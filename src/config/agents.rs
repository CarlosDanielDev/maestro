use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    Qwen,
    Opencode,
    Ollama,
    Minimax,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Qwen => "qwen",
            Self::Opencode => "opencode",
            Self::Ollama => "ollama",
            Self::Minimax => "minimax",
        }
    }

    pub fn is_subprocess(self) -> bool {
        matches!(
            self,
            Self::Claude | Self::Codex | Self::Qwen | Self::Opencode
        )
    }

    pub fn is_http(self) -> bool {
        matches!(self, Self::Ollama | Self::Minimax)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "AgentConfigRaw")]
pub struct AgentConfig {
    pub kind: AgentKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    /// Claude-only: how to drive the `claude` CLI. `"headless"` (default) is
    /// the one-shot `--print` path; `"interactive"` runs the REPL on a PTY so
    /// Pro/Max subscription billing survives the 2026-06-15 cutoff (#750).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config_overrides: BTreeMap<String, toml::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cli_flags: BTreeMap<String, toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Ollama-only: context window in tokens, surfaced in the TUI's
    /// context-fill gauge. Ignored for non-Ollama agents. Unset → gauge
    /// is hidden for that session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u32>,
}

impl AgentConfig {
    pub fn builtin_claude(
        model: impl Into<String>,
        permission_mode: impl Into<String>,
        allowed_tools: Vec<String>,
    ) -> Self {
        Self {
            kind: AgentKind::Claude,
            enabled: true,
            command: Some("claude".to_string()),
            base_url: None,
            model: Some(model.into()),
            env: BTreeMap::new(),
            extra_args: Vec::new(),
            permission_mode: Some(permission_mode.into()),
            allowed_tools,
            transport: None,
            sandbox: None,
            json: None,
            ephemeral: None,
            profile: None,
            config_overrides: BTreeMap::new(),
            cli_flags: BTreeMap::new(),
            request_timeout_secs: None,
            api_key_env: None,
            num_ctx: None,
        }
    }

    pub fn validate(&self, id: &str) -> Result<()> {
        validate_agent_id(id)?;

        if self.kind.is_subprocess() {
            let command = self.command.as_deref().unwrap_or("").trim();
            if command.is_empty() {
                anyhow::bail!(
                    "agents.{id}.command is required for {} agents",
                    self.kind.as_str()
                );
            }
            // TUI text inputs round-trip cleared fields as `Some("")`,
            // not `None`. Treat empty / whitespace-only base_url as
            // "not set" so the user can recover from a stale HTTP-era
            // value by simply clearing the field.
            let base_url_set = self
                .base_url
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if base_url_set {
                anyhow::bail!(
                    "agents.{id}.base_url is only valid for HTTP agents; remove it from {} agent `{id}`",
                    self.kind.as_str()
                );
            }
        }

        if self.kind.is_http() {
            let base_url = self.base_url.as_deref().unwrap_or("").trim();
            if base_url.is_empty() {
                anyhow::bail!(
                    "agents.{id}.base_url is required for {} agents",
                    self.kind.as_str()
                );
            }
            // Same empty-string handling as the subprocess branch.
            let command_set = self
                .command
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if command_set {
                anyhow::bail!(
                    "agents.{id}.command is only valid for subprocess agents; remove it from {} agent `{id}`",
                    self.kind.as_str()
                );
            }
        }

        // Claude-only transport selector (#750). Same empty-string-as-unset
        // handling as the other optional fields.
        if let Some(transport) = self
            .transport
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if self.kind != AgentKind::Claude {
                anyhow::bail!(
                    "agents.{id}.transport is only valid for claude agents; remove it from {} agent `{id}`",
                    self.kind.as_str()
                );
            }
            if !matches!(transport, "headless" | "interactive") {
                anyhow::bail!(
                    "agents.{id}.transport `{transport}`: unknown transport, expected one of: headless, interactive"
                );
            }
        }

        // Same empty-string-as-unset handling as base_url / command above.
        // An empty TextInput round-trips as `Some("")`; treat that as "no
        // api_key_env was specified" so users can clear the field in the
        // TUI without tripping the env-var-name validator.
        if let Some(api_key_env) = self
            .api_key_env
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            validate_env_var_name(api_key_env)
                .map_err(|msg| anyhow::anyhow!("agents.{id}.api_key_env {msg}"))?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct AgentConfigRaw {
    kind: AgentKind,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    extra_args: Vec<String>,
    #[serde(default)]
    permission_mode: Option<String>,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    sandbox: Option<String>,
    #[serde(default)]
    json: Option<bool>,
    #[serde(default)]
    ephemeral: Option<bool>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    config_overrides: BTreeMap<String, toml::Value>,
    #[serde(default)]
    cli_flags: BTreeMap<String, toml::Value>,
    #[serde(default)]
    request_timeout_secs: Option<u64>,
    #[serde(default)]
    api_key_env: Option<String>,
    #[serde(default)]
    num_ctx: Option<u32>,
}

impl From<AgentConfigRaw> for AgentConfig {
    fn from(raw: AgentConfigRaw) -> Self {
        let command = raw.command.or_else(|| match raw.kind {
            AgentKind::Codex => Some("codex".to_string()),
            AgentKind::Qwen => Some("qwen".to_string()),
            AgentKind::Opencode => Some("opencode".to_string()),
            _ => None,
        });
        let base_url = raw.base_url.or_else(|| match raw.kind {
            AgentKind::Ollama => Some("http://localhost:11434".to_string()),
            AgentKind::Minimax => Some("https://api.minimax.io/v1".to_string()),
            _ => None,
        });
        let model = raw.model.or_else(|| match raw.kind {
            AgentKind::Minimax => Some("MiniMax-M2.7".to_string()),
            _ => None,
        });
        let request_timeout_secs = raw.request_timeout_secs.or(match raw.kind {
            AgentKind::Ollama | AgentKind::Minimax => Some(120),
            _ => None,
        });
        let api_key_env = raw.api_key_env.or_else(|| match raw.kind {
            AgentKind::Minimax => Some("MINIMAX_API_KEY".to_string()),
            _ => None,
        });

        Self {
            kind: raw.kind,
            enabled: raw.enabled,
            command,
            base_url,
            model,
            env: raw.env,
            extra_args: raw.extra_args,
            permission_mode: raw.permission_mode,
            allowed_tools: raw.allowed_tools,
            transport: raw.transport,
            sandbox: raw.sandbox,
            json: raw.json,
            ephemeral: raw.ephemeral,
            profile: raw.profile,
            config_overrides: raw.config_overrides,
            cli_flags: raw.cli_flags,
            request_timeout_secs,
            api_key_env,
            num_ctx: raw.num_ctx,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(default = "default_agent_id")]
    pub default: String,
    #[serde(flatten)]
    pub entries: BTreeMap<String, AgentConfig>,
}

impl AgentsConfig {
    pub fn is_default(&self) -> bool {
        self.default == default_agent_id() && self.entries.is_empty()
    }

    pub fn validate(&self) -> Result<()> {
        if self.entries.is_empty() {
            if self.default != default_agent_id() {
                anyhow::bail!("agents.default `{}` is not configured", self.default);
            }
            return Ok(());
        }

        for (id, agent) in &self.entries {
            agent.validate(id)?;
        }

        let Some(agent) = self.entries.get(&self.default) else {
            anyhow::bail!("agents.default `{}` is not configured", self.default);
        };
        if !agent.enabled {
            anyhow::bail!("agents.default `{}` is disabled", self.default);
        }
        Ok(())
    }
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            default: default_agent_id(),
            entries: BTreeMap::new(),
        }
    }
}

fn validate_agent_id(id: &str) -> Result<()> {
    if id.trim().is_empty() {
        anyhow::bail!("agent id must not be empty");
    }
    if id.chars().any(char::is_control) {
        anyhow::bail!("agent id `{id}` must not contain control characters");
    }
    Ok(())
}

fn validate_env_var_name(name: &str) -> std::result::Result<(), &'static str> {
    if name.is_empty() {
        return Err("must not be empty");
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("must not be empty");
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err("must be an environment variable name");
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err("must be an environment variable name");
    }
    Ok(())
}

fn default_agent_id() -> String {
    "claude".to_string()
}

fn default_true() -> bool {
    true
}
