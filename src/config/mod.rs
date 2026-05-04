use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

mod adapt;
mod budget;
mod flags;
mod gates;
mod github;
mod models;
mod modes;
mod notifications;
mod plugins;
mod project;
mod review;
mod runtime;
mod sessions;
mod tui;
mod turboquant;
mod views;

pub use adapt::{AdaptSettings, MilestoneNaming};
pub use budget::BudgetConfig;
pub use flags::FlagsConfig;
#[allow(unused_imports)]
pub use gates::{CiAutoFixConfig, GatesConfig};
#[allow(unused_imports)]
pub use github::{GithubConfig, MergeMethod, ProviderConfig};
pub use models::ModelsConfig;
pub use modes::ModeConfig;
pub use notifications::NotificationsConfig;
pub use plugins::PluginConfig;
pub use project::ProjectConfig;
#[allow(unused_imports)]
pub use review::{ReviewConfig, ReviewerEntry};
pub use runtime::{ConcurrencyConfig, MonitoringConfig};
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
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self).context("serializing config to TOML")?;
        std::fs::write(path, content)
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
