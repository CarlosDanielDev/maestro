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
    /// Permission mode for Claude CLI sessions.
    /// Options: "default", "acceptEdits", "bypassPermissions", "dontAsk", "plan", "auto"
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    /// Allowed tools whitelist (comma-separated). Empty = all tools allowed.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Maximum number of retries for failed/stalled sessions.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Cooldown in seconds between retries.
    #[serde(default = "default_retry_cooldown")]
    pub retry_cooldown_secs: u64,
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
    /// Cache TTL for issue data in seconds. Default: 300 (5 min).
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    /// Whether to auto-merge PRs after all gates pass. Default: false.
    #[serde(default)]
    pub auto_merge: bool,
    /// Merge method. Default: Squash.
    #[serde(default)]
    pub merge_method: MergeMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default)]
    pub slack: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    Merge,
    #[default]
    Squash,
    Rebase,
}

impl MergeMethod {
    pub fn flag(&self) -> &'static str {
        match self {
            Self::Merge => "--merge",
            Self::Squash => "--squash",
            Self::Rebase => "--rebase",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatesConfig {
    /// Whether gates are enabled. Default: true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Test command to run as the default gate. Default: "cargo test".
    #[serde(default = "default_test_command")]
    pub test_command: String,
    /// Interval in seconds between CI status polls. Default: 30.
    #[serde(default = "default_ci_poll_interval")]
    pub ci_poll_interval_secs: u64,
    /// Maximum time in seconds to wait for CI to complete. Default: 1800 (30min).
    #[serde(default = "default_ci_max_wait")]
    pub ci_max_wait_secs: u64,
}

impl Default for GatesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            test_command: default_test_command(),
            ci_poll_interval_secs: default_ci_poll_interval(),
            ci_max_wait_secs: default_ci_max_wait(),
        }
    }
}

fn default_test_command() -> String {
    "cargo test".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Whether review dispatch is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Review command template (used when `reviewers` is empty). Variables: {pr_number}, {branch}.
    #[serde(default = "default_review_command")]
    pub command: String,
    /// Whether to auto-approve PRs after successful review.
    #[serde(default)]
    pub auto_approve: bool,
    /// Multi-reviewer council configuration. If non-empty, overrides `command`.
    #[serde(default)]
    pub reviewers: Vec<ReviewerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerEntry {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub required: bool,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: default_review_command(),
            auto_approve: false,
            reviewers: Vec::new(),
        }
    }
}

fn default_review_command() -> String {
    "gh pr review {pr_number} --comment --body 'Automated review by Maestro'".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// Routing rules: label pattern -> model name. First match wins.
    /// Example: { "priority:P0" = "opus", "type:docs" = "haiku" }
    #[serde(default)]
    pub routing: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Labels that mark a task as "heavy" (resource-intensive).
    #[serde(default)]
    pub heavy_task_labels: Vec<String>,
    /// Maximum number of heavy tasks that can run concurrently.
    #[serde(default = "default_heavy_task_limit")]
    pub heavy_task_limit: usize,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            heavy_task_labels: Vec::new(),
            heavy_task_limit: default_heavy_task_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Interval in seconds for work assigner ticks. Default: 10.
    #[serde(default = "default_work_tick_interval")]
    pub work_tick_interval_secs: u64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            work_tick_interval_secs: default_work_tick_interval(),
        }
    }
}

/// Plugin configuration: shell commands triggered on hook events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Display name for the plugin.
    pub name: String,
    /// Hook point to trigger on (e.g., "session_completed", "pr_created").
    pub on: String,
    /// Shell command to execute.
    pub run: String,
    /// Per-plugin timeout override in seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Mode configuration: defines system prompt and allowed tools for a named mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeConfig {
    /// System prompt override for this mode.
    #[serde(default)]
    pub system_prompt: String,
    /// Allowed tools whitelist. Empty = all tools.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Permission mode override for this mode.
    #[serde(default)]
    pub permission_mode: Option<String>,
}

fn default_heavy_task_limit() -> usize {
    2
}
fn default_work_tick_interval() -> u64 {
    10
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
fn default_permission_mode() -> String {
    "bypassPermissions".into()
}
fn default_max_retries() -> u32 {
    2
}
fn default_retry_cooldown() -> u64 {
    60
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
fn default_cache_ttl() -> u64 {
    300
}
fn default_true() -> bool {
    true
}
fn default_ci_poll_interval() -> u64 {
    30
}
fn default_ci_max_wait() -> u64 {
    1800
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
