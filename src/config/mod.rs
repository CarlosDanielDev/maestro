use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

mod adapt;
mod agents;
mod agents_upgrade;
mod budget;
mod experimental;
mod flags;
mod gates;
mod github;
mod io;
mod migrate;
mod models;
mod modes;
mod notifications;
mod overlay;
mod plugins;
mod project;
mod review;
mod runtime;
mod sessions;
mod tui;
mod turboquant;
mod views;

pub use adapt::{AdaptSettings, MilestoneNaming};
pub use agents::{AgentConfig, AgentKind, AgentsConfig};
#[allow(unused_imports)]
pub use agents_upgrade::{AgentConfigUpgradePlan, AgentConfigVersion, plan_agent_config_upgrade};
pub use budget::BudgetConfig;
pub use experimental::ExperimentalConfig;
pub use flags::FlagsConfig;
#[allow(unused_imports)]
pub use gates::{CiAutoFixConfig, GatesConfig};
#[allow(unused_imports)]
pub use github::{GithubConfig, MergeMethod, ProviderConfig};
pub use migrate::run_startup_migration;
pub use models::ModelsConfig;
pub use modes::ModeConfig;
pub use notifications::NotificationsConfig;
pub use plugins::PluginConfig;
pub use project::ProjectConfig;
#[allow(unused_imports)]
pub use review::{ReviewConfig, ReviewerEntry};
pub use runtime::{ConcurrencyConfig, MonitoringConfig};
#[allow(unused_imports)]
pub(crate) use sessions::default_max_prompt_history;
#[allow(unused_imports)]
pub use sessions::{
    CompletionGateEntry, CompletionGatesConfig, ConflictConfig, ConflictPolicy,
    ContextOverflowConfig, HollowRetryConfig, HollowRetryPolicy, SessionsConfig,
};
pub use tui::{Density, LayoutConfig, LayoutMode, TuiConfig};
pub use turboquant::{ApplyTarget, QuantStrategy, TurboQuantConfig};
pub use views::ViewsConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub project: ProjectConfig,
    pub sessions: SessionsConfig,
    pub budget: BudgetConfig,
    #[serde(default)]
    pub github: GithubConfig,
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub models: ModelsConfig,
    #[serde(default)]
    pub gates: GatesConfig,
    #[serde(default)]
    pub review: ReviewConfig,
    #[serde(default)]
    pub concurrency: ConcurrencyConfig,
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    #[serde(default)]
    pub plugins: Vec<PluginConfig>,
    #[serde(default)]
    pub modes: std::collections::HashMap<String, ModeConfig>,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub flags: FlagsConfig,
    #[serde(default)]
    pub turboquant: TurboQuantConfig,
    #[serde(default)]
    pub adapt: AdaptSettings,
    #[serde(default)]
    pub views: ViewsConfig,
    #[serde(default, skip_serializing_if = "AgentsConfig::is_default")]
    pub agents: AgentsConfig,
    #[serde(default, skip_serializing_if = "ExperimentalConfig::is_default")]
    pub experimental: ExperimentalConfig,
    #[serde(default)]
    pub teams: HashMap<String, crate::orchestration::team::TeamConfig>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Self =
            toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
        if has_legacy_azure_devops_flag(&content)
            && config.provider.kind != crate::provider::types::ProviderKind::AzureDevops
        {
            log_legacy_azure_devops_config(false);
        }
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        match std::fs::metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.save_fresh(path),
            Err(e) => Err(e).with_context(|| format!("stat config path {}", path.display())),
            Ok(meta) if meta.len() == 0 => self.save_fresh(path),
            Ok(_) => self.save_into_existing(path),
        }
    }

    /// Whole-document write used when the destination does not yet exist
    /// (fresh install). Preserves prior behavior; no overlay involved.
    fn save_fresh(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self).context("serializing config to TOML")?;
        io::atomic_write(path, &content)
            .with_context(|| format!("writing config to {}", path.display()))?;
        Ok(())
    }

    /// Comment-preserving save: applies the in-memory `Config`'s changes onto
    /// the existing TOML text using a `toml_edit::DocumentMut` overlay, so
    /// comments, blank lines, key order, and unknown sections survive.
    /// Pure (no I/O); returns the new file contents.
    pub fn save_into_str(&self, original_text: &str) -> Result<String> {
        let mut existing: toml_edit::DocumentMut = original_text.parse().with_context(|| {
            "parsing existing config as TOML (refusing to overwrite malformed file)".to_string()
        })?;
        let on_disk_cfg: Config =
            toml::from_str(original_text).context("parsing existing config into Config model")?;

        // Route both serializations through `toml::to_string_pretty` so that
        // nested struct fields render as standard `[section]` headers rather
        // than inline tables — `toml_edit::ser::to_document` defaults to
        // inline-table form for nested struct fields, which would force every
        // section change to be a wholesale type-mismatch replacement.
        let on_disk_canonical = toml::to_string(&on_disk_cfg)
            .context("serializing on-disk config to canonical TOML")?;
        let new_canonical =
            toml::to_string(self).context("serializing in-memory config to canonical TOML")?;
        let on_disk_doc: toml_edit::DocumentMut = on_disk_canonical
            .parse()
            .context("re-parsing canonical on-disk config")?;
        let new_doc: toml_edit::DocumentMut = new_canonical
            .parse()
            .context("re-parsing canonical in-memory config")?;

        overlay::apply_overlay(&mut existing, on_disk_doc.as_table(), new_doc.as_table());

        Ok(existing.to_string())
    }

    /// Atomic save over an existing on-disk file using the overlay.
    /// Refuses to overwrite a malformed file. Mirror of `save_into_str`'s
    /// guarantees, plus tempfile-based atomic write.
    pub fn save_into_existing(&self, path: &Path) -> Result<()> {
        let original = std::fs::read_to_string(path)
            .with_context(|| format!("reading existing config at {}", path.display()))?;
        let new_content = self.save_into_str(&original)?;
        io::atomic_write(path, &new_content)
            .with_context(|| format!("writing config to {}", path.display()))?;
        Ok(())
    }

    pub fn find_and_load() -> Result<Self> {
        Self::find_and_load_with_path().map(|lc| lc.config)
    }

    /// Find `maestro.toml` or `.maestro/config.toml` under `base` and load it.
    pub fn find_and_load_in(base: &Path) -> Result<Self> {
        Self::find_and_load_in_with_path(base).map(|lc| lc.config)
    }

    /// Locate a config file and return both the parsed `Config` and the path
    /// it was loaded from. Prefer this at TUI entry points that need to save
    /// back to the same file.
    pub fn find_and_load_with_path() -> Result<LoadedConfig> {
        Self::find_and_load_in_with_path(Path::new("."))
    }

    pub fn find_and_load_in_with_path(base: &Path) -> Result<LoadedConfig> {
        for candidate in ["maestro.toml", ".maestro/config.toml"] {
            let path = base.join(candidate);
            match Self::load(&path) {
                Ok(config) => {
                    let resolved = std::fs::canonicalize(&path).unwrap_or(path);
                    return Ok(LoadedConfig {
                        config,
                        path: resolved,
                    });
                }
                Err(e) => {
                    // Only keep searching if this particular file doesn't exist.
                    if let Some(io_err) = e.downcast_ref::<std::io::Error>()
                        && io_err.kind() == std::io::ErrorKind::NotFound
                    {
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        anyhow::bail!(
            "No maestro.toml found under {}. Run `maestro init` to create one.",
            base.display()
        )
    }

    pub fn validate(&self) -> Result<()> {
        if self.provider.kind == crate::provider::types::ProviderKind::AzureDevops {
            log_legacy_azure_devops_config(self.experimental.azure_devops);
            let organization = self.provider.organization.as_deref().unwrap_or("").trim();
            if !valid_azure_devops_organization_url(organization) {
                anyhow::bail!(
                    "provider.organization must be https://dev.azure.com/<org> or https://<org>.visualstudio.com for azure_devops"
                );
            }
            let project = self.provider.az_project.as_deref().unwrap_or("").trim();
            if project.is_empty() || project.chars().any(char::is_control) {
                anyhow::bail!("provider.az_project is required for azure_devops");
            }
        }

        self.agents.validate()?;

        Ok(())
    }

    pub fn resolve_agent(&self, agent_id: Option<&str>) -> Result<ResolvedAgentConfig> {
        let id = agent_id.unwrap_or(self.agents.default.as_str());
        if self.agents.entries.is_empty() {
            if id != "claude" {
                anyhow::bail!("agent `{id}` is not configured; no [agents] table is present");
            }
            return Ok(ResolvedAgentConfig {
                id: "claude".to_string(),
                config: AgentConfig::builtin_claude(
                    self.sessions.default_model.clone(),
                    self.sessions.permission_mode.clone(),
                    self.sessions.allowed_tools.clone(),
                ),
            });
        }

        let Some(agent) = self.agents.entries.get(id) else {
            anyhow::bail!("agent `{id}` is not configured");
        };
        if !agent.enabled {
            anyhow::bail!("agent `{id}` is disabled");
        }
        agent.validate(id)?;

        let mut config = agent.clone();
        if config.model.as_deref().unwrap_or("").trim().is_empty()
            && config.kind == AgentKind::Claude
        {
            config.model = Some(self.sessions.default_model.clone());
        }
        if config.permission_mode.is_none()
            && matches!(
                config.kind,
                AgentKind::Claude | AgentKind::Codex | AgentKind::Qwen
            )
        {
            config.permission_mode = Some(self.sessions.permission_mode.clone());
        }
        if config.allowed_tools.is_empty() && config.kind == AgentKind::Claude {
            config.allowed_tools = self.sessions.allowed_tools.clone();
        }

        Ok(ResolvedAgentConfig {
            id: id.to_string(),
            config,
        })
    }

    pub fn effective_provider_config(&self) -> ProviderConfig {
        let mut provider = self.provider.clone();
        if provider.repo.as_deref().unwrap_or("").trim().is_empty()
            && !self.project.repo.trim().is_empty()
        {
            provider.repo = Some(self.project.repo.clone());
        }
        provider
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAgentConfig {
    pub id: String,
    pub config: AgentConfig,
}

fn has_legacy_azure_devops_flag(content: &str) -> bool {
    let Ok(value) = content.parse::<toml::Value>() else {
        return false;
    };
    matches!(
        value
            .get("experimental")
            .and_then(|experimental| experimental.get("azure_devops"))
            .and_then(toml::Value::as_bool),
        Some(false)
    )
}

fn log_legacy_azure_devops_config(azure_devops: bool) {
    if !azure_devops {
        tracing::debug!(
            "`experimental.azure_devops = false` is retained for backward compatibility; \
             Azure DevOps is stable and no startup gate is applied"
        );
    }
}

fn valid_azure_devops_organization_url(input: &str) -> bool {
    let dev_azure = Regex::new(r"^https://dev\.azure\.com/[^/]+$").expect("valid regex");
    let visualstudio = Regex::new(r"^https://[^/]+\.visualstudio\.com$").expect("valid regex");
    !input.chars().any(char::is_control)
        && (dev_azure.is_match(input) || visualstudio.is_match(input))
}

/// A `Config` bundled with the filesystem path it was loaded from.
/// Propagated from boot to the Settings screen so `Ctrl+s` writes back to
/// the same file, regardless of later CWD changes.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: std::path::PathBuf,
}

#[cfg(test)]
mod tests;
