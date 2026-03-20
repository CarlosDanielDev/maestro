use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub project: ProjectConfig,
    pub sessions: SessionsConfig,
    pub budget: BudgetConfig,
    pub github: GithubConfig,
    pub notifications: NotificationsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub repo: String,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_stall_timeout")]
    pub stall_timeout_secs: u64,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default = "default_mode")]
    pub default_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    #[serde(default = "default_per_session_usd")]
    pub per_session_usd: f64,
    #[serde(default = "default_total_usd")]
    pub total_usd: f64,
    #[serde(default = "default_alert_threshold")]
    pub alert_threshold_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubConfig {
    #[serde(default = "default_issue_labels")]
    pub issue_filter_labels: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_pr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default)]
    pub slack: bool,
}

fn default_base_branch() -> String {
    "main".into()
}
fn default_max_concurrent() -> usize {
    3
}
fn default_stall_timeout() -> u64 {
    300
}
fn default_model() -> String {
    "opus".into()
}
fn default_mode() -> String {
    "orchestrator".into()
}
fn default_per_session_usd() -> f64 {
    5.0
}
fn default_total_usd() -> f64 {
    50.0
}
fn default_alert_threshold() -> u8 {
    80
}
fn default_issue_labels() -> Vec<String> {
    vec!["maestro:ready".into()]
}
fn default_true() -> bool {
    true
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn find_and_load() -> Result<Self> {
        let candidates = [
            PathBuf::from("maestro.toml"),
            PathBuf::from(".maestro/config.toml"),
        ];
        for path in &candidates {
            if path.exists() {
                return Self::load(path);
            }
        }
        anyhow::bail!("No maestro.toml found. Run `maestro init` to create one.")
    }
}
