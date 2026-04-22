use crate::provider::types::ProviderKind;
use crate::tui::theme::ThemeConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlagsConfig {
    #[serde(flatten)]
    pub entries: HashMap<String, bool>,
}

/// Milestone naming convention for the adapt pipeline.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MilestoneNaming {
    /// Infer naming from existing milestones (e.g. semver `vX.X.X`).
    Standard,
    /// Let Claude decide scope and naming (default: `MN: Description`).
    #[default]
    Ai,
    /// User-provided template pattern.
    Custom,
}

/// Configuration for the `maestro adapt` command.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptSettings {
    /// How milestones should be named in the generated plan.
    #[serde(default)]
    pub milestone_naming: MilestoneNaming,
    /// Custom template for milestone names (only used when `milestone_naming = "custom"`).
    /// Supports `{n}` (index) and `{title}` (description) placeholders.
    #[serde(default)]
    pub milestone_template: Option<String>,
}

pub use crate::turboquant::types::{ApplyTarget, QuantStrategy};

/// TurboQuant quantization configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurboQuantConfig {
    /// Whether TurboQuant quantization is active.
    #[serde(default)]
    pub enabled: bool,
    /// Bit width for quantization (1-8).
    #[serde(default = "default_turbo_bit_width")]
    pub bit_width: u8,
    /// Quantization strategy.
    #[serde(default)]
    pub strategy: QuantStrategy,
    /// Which components to compress.
    #[serde(default)]
    pub apply_to: ApplyTarget,
    /// Automatically enable on context overflow events.
    #[serde(default)]
    pub auto_on_overflow: bool,
    /// Token budget for fork-handoff compression.
    #[serde(default = "default_fork_handoff_budget")]
    pub fork_handoff_budget: usize,
    /// Token budget for system-prompt compaction.
    #[serde(default = "default_system_prompt_budget")]
    pub system_prompt_budget: usize,
    /// Token budget for knowledge-base compression.
    #[serde(default = "default_knowledge_budget")]
    pub knowledge_budget: usize,
}

fn default_turbo_bit_width() -> u8 {
    4
}

fn default_fork_handoff_budget() -> usize {
    4096
}

fn default_system_prompt_budget() -> usize {
    2048
}

fn default_knowledge_budget() -> usize {
    4096
}

impl Default for TurboQuantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bit_width: default_turbo_bit_width(),
            strategy: QuantStrategy::default(),
            apply_to: ApplyTarget::default(),
            auto_on_overflow: false,
            fork_handoff_budget: default_fork_handoff_budget(),
            system_prompt_budget: default_system_prompt_budget(),
            knowledge_budget: default_knowledge_budget(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub ascii_icons: bool,
    /// Show the Clawd mascot companion in the TUI.
    #[serde(default = "default_show_mascot")]
    pub show_mascot: bool,
}

fn default_show_mascot() -> bool {
    true
}

/// Layout configuration for the Issues screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Panel arrangement mode.
    #[serde(default)]
    pub mode: LayoutMode,
    /// Information density level.
    #[serde(default)]
    pub density: Density,
    /// Percentage of width (horizontal) or height (vertical) for preview panel.
    #[serde(default = "default_preview_ratio")]
    pub preview_ratio: u8,
    /// Percentage of height for activity log panel.
    #[serde(default = "default_activity_log_height")]
    pub activity_log_height: u8,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::default(),
            density: Density::default(),
            preview_ratio: default_preview_ratio(),
            activity_log_height: default_activity_log_height(),
        }
    }
}

/// Panel arrangement mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    #[default]
    Vertical,
    Horizontal,
}

/// Information density level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Default,
    Comfortable,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub repo: String,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
}

/// Policy variant for retrying hollow-completion sessions (#275).
///
/// The label is serialized as kebab-case in TOML
/// (e.g. `policy = "intent-aware"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HollowRetryPolicy {
    /// Always retry hollow completions up to `work_max_retries`
    /// (consultation intent is ignored — one knob governs everything).
    Always,
    /// Route by `SessionIntent`: work sessions use `work_max_retries`,
    /// consultation sessions use `consultation_max_retries`. Default.
    #[default]
    IntentAware,
    /// Never retry a hollow completion, regardless of intent.
    Never,
}

/// Hollow-completion retry configuration. See `[sessions.hollow_retry]`
/// in `maestro.toml`. Default: `IntentAware` with `work = 2`, `consultation = 0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HollowRetryConfig {
    #[serde(default)]
    pub policy: HollowRetryPolicy,
    #[serde(default = "default_work_max_retries")]
    pub work_max_retries: u32,
    #[serde(default = "default_consultation_max_retries")]
    pub consultation_max_retries: u32,
}

impl Default for HollowRetryConfig {
    fn default() -> Self {
        Self {
            policy: HollowRetryPolicy::default(),
            work_max_retries: default_work_max_retries(),
            consultation_max_retries: default_consultation_max_retries(),
        }
    }
}

/// Merge a legacy flat `sessions.hollow_max_retries = N` value into the
/// new `[sessions.hollow_retry]` section. When both are present, the new
/// section wins and a one-shot deprecation warning is logged.
fn merge_legacy_hollow(
    new_section: Option<HollowRetryConfig>,
    legacy_flat: Option<u32>,
) -> HollowRetryConfig {
    match (new_section, legacy_flat) {
        (Some(new), Some(_)) => {
            tracing::warn!(
                "both `sessions.hollow_max_retries` (legacy) and `[sessions.hollow_retry]` \
                 are set in maestro.toml; the new section takes precedence. \
                 Remove `hollow_max_retries` to silence this warning."
            );
            new
        }
        (Some(new), None) => new,
        (None, Some(n)) => HollowRetryConfig {
            policy: HollowRetryPolicy::IntentAware,
            work_max_retries: n,
            consultation_max_retries: 0,
        },
        (None, None) => HollowRetryConfig::default(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "SessionsConfigRaw")]
pub struct SessionsConfig {
    pub max_concurrent: usize,
    pub stall_timeout_secs: u64,
    pub default_model: String,
    pub default_mode: String,
    /// Permission mode for Claude CLI sessions.
    /// Options: "default", "acceptEdits", "bypassPermissions", "dontAsk", "plan", "auto"
    pub permission_mode: String,
    /// Allowed tools whitelist (comma-separated). Empty = all tools allowed.
    pub allowed_tools: Vec<String>,
    /// Maximum number of retries for failed/stalled sessions.
    pub max_retries: u32,
    /// Cooldown in seconds between retries.
    pub retry_cooldown_secs: u64,
    /// Hollow-completion retry policy (#275).
    pub hollow_retry: HollowRetryConfig,
    /// Maximum number of prompt history entries to retain. Default: 100.
    pub max_prompt_history: usize,
    /// Context overflow detection and auto-fork configuration.
    pub context_overflow: ContextOverflowConfig,
    /// Conflict detection policy configuration.
    pub conflict: ConflictConfig,
    /// Custom guardrail prompt injected into every session's system prompt.
    /// If unset, a default is auto-detected based on project language.
    pub guardrail_prompt: Option<String>,
    /// Completion gates that run after session finishes, before PR creation.
    pub completion_gates: CompletionGatesConfig,
}

/// Shadow struct used only for deserialization. Mirrors `SessionsConfig`
/// but accepts both the legacy flat `hollow_max_retries` field and the new
/// `hollow_retry` section. The custom `Deserialize` impl for
/// `SessionsConfig` reconciles them via `merge_legacy_hollow`.
#[derive(Deserialize)]
struct SessionsConfigRaw {
    #[serde(default = "default_max_concurrent")]
    max_concurrent: usize,
    #[serde(default = "default_stall_timeout")]
    stall_timeout_secs: u64,
    #[serde(default = "default_model")]
    default_model: String,
    #[serde(default = "default_mode")]
    default_mode: String,
    #[serde(default = "default_permission_mode")]
    permission_mode: String,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default = "default_max_retries")]
    max_retries: u32,
    #[serde(default = "default_retry_cooldown")]
    retry_cooldown_secs: u64,
    #[serde(default)]
    hollow_max_retries: Option<u32>,
    #[serde(default)]
    hollow_retry: Option<HollowRetryConfig>,
    #[serde(default = "default_max_prompt_history")]
    max_prompt_history: usize,
    #[serde(default)]
    context_overflow: ContextOverflowConfig,
    #[serde(default)]
    conflict: ConflictConfig,
    #[serde(default)]
    guardrail_prompt: Option<String>,
    #[serde(default)]
    completion_gates: CompletionGatesConfig,
}

impl From<SessionsConfigRaw> for SessionsConfig {
    fn from(raw: SessionsConfigRaw) -> Self {
        Self {
            max_concurrent: raw.max_concurrent,
            stall_timeout_secs: raw.stall_timeout_secs,
            default_model: raw.default_model,
            default_mode: raw.default_mode,
            permission_mode: raw.permission_mode,
            allowed_tools: raw.allowed_tools,
            max_retries: raw.max_retries,
            retry_cooldown_secs: raw.retry_cooldown_secs,
            hollow_retry: merge_legacy_hollow(raw.hollow_retry, raw.hollow_max_retries),
            max_prompt_history: raw.max_prompt_history,
            context_overflow: raw.context_overflow,
            conflict: raw.conflict,
            guardrail_prompt: raw.guardrail_prompt,
            completion_gates: raw.completion_gates,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionGatesConfig {
    /// Whether completion gates are enabled. Default: true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Ordered list of gate commands to run.
    #[serde(default)]
    pub commands: Vec<CompletionGateEntry>,
}

impl Default for CompletionGatesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionGateEntry {
    /// Display name for activity log (e.g., "fmt", "clippy").
    pub name: String,
    /// Shell command to execute. Exit code 0 = pass.
    pub run: String,
    /// If true, failure blocks PR creation. Default: true.
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextOverflowConfig {
    /// Context usage percentage at which auto-fork triggers. Default: 70.
    #[serde(default = "default_overflow_threshold_pct")]
    pub overflow_threshold_pct: u8,
    /// Whether auto-fork is enabled. Default: true.
    #[serde(default = "default_true")]
    pub auto_fork: bool,
    /// Context percentage at which to prompt a periodic commit. Default: 50.
    #[serde(default = "default_commit_prompt_pct")]
    pub commit_prompt_pct: u8,
    /// Maximum depth of fork chains to prevent runaway forking. Default: 5.
    #[serde(default = "default_max_fork_depth")]
    pub max_fork_depth: u8,
}

impl ContextOverflowConfig {
    /// Overflow threshold as a 0.0-1.0 ratio.
    pub fn overflow_ratio(&self) -> f64 {
        self.overflow_threshold_pct as f64 / 100.0
    }

    /// Commit prompt threshold as a 0.0-1.0 ratio.
    pub fn commit_prompt_ratio(&self) -> f64 {
        self.commit_prompt_pct as f64 / 100.0
    }
}

impl Default for ContextOverflowConfig {
    fn default() -> Self {
        Self {
            overflow_threshold_pct: default_overflow_threshold_pct(),
            auto_fork: true,
            commit_prompt_pct: default_commit_prompt_pct(),
            max_fork_depth: default_max_fork_depth(),
        }
    }
}

/// Policy to enforce when a file conflict is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// Log a warning but allow the session to continue.
    #[default]
    Warn,
    /// Pause the offending session (SIGSTOP on Unix).
    Pause,
    /// Kill the offending session immediately.
    Kill,
}

impl ConflictPolicy {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Pause => "pause",
            Self::Kill => "kill",
        }
    }
}

/// Conflict detection configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictConfig {
    /// Whether real-time conflict detection is enabled. Default: true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Policy to enforce on conflict. Default: warn.
    #[serde(default)]
    pub policy: ConflictPolicy,
}

impl Default for ConflictConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            policy: ConflictPolicy::Warn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetConfig {
    #[serde(default = "default_per_session_usd")]
    pub per_session_usd: f64,
    #[serde(default = "default_total_usd")]
    pub total_usd: f64,
    #[serde(default = "default_alert_threshold")]
    pub alert_threshold_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            issue_filter_labels: default_issue_labels(),
            auto_pr: true,
            cache_ttl_secs: default_cache_ttl(),
            auto_merge: false,
            merge_method: MergeMethod::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type: "github" or "azure_devops". Default: github.
    #[serde(default)]
    pub kind: ProviderKind,
    /// Issue/work-item filter labels/tags.
    #[serde(default = "default_issue_labels")]
    pub issue_filter_labels: Vec<String>,
    /// Whether to auto-create PRs on session completion.
    #[serde(default = "default_true")]
    pub auto_pr: bool,
    /// Whether to auto-merge PRs after gates pass.
    #[serde(default)]
    pub auto_merge: bool,
    /// Merge method.
    #[serde(default)]
    pub merge_method: MergeMethod,
    /// Cache TTL for issue data in seconds.
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    /// Azure DevOps organization URL (e.g., "https://dev.azure.com/MyOrg").
    #[serde(default)]
    pub organization: Option<String>,
    /// Azure DevOps project name.
    #[serde(default)]
    pub az_project: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            kind: ProviderKind::default(),
            issue_filter_labels: default_issue_labels(),
            auto_pr: true,
            auto_merge: false,
            merge_method: MergeMethod::default(),
            cache_ttl_secs: default_cache_ttl(),
            organization: None,
            az_project: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default)]
    pub slack: bool,
    /// Slack webhook URL for sending notifications.
    #[serde(default)]
    pub slack_webhook_url: Option<String>,
    /// Maximum Slack messages per minute (rate limiting). Default: 10.
    #[serde(default = "default_slack_rate_limit")]
    pub slack_rate_limit_per_min: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// CI auto-fix loop configuration.
    #[serde(default)]
    pub ci_auto_fix: CiAutoFixConfig,
}

impl Default for GatesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            test_command: default_test_command(),
            ci_poll_interval_secs: default_ci_poll_interval(),
            ci_max_wait_secs: default_ci_max_wait(),
            ci_auto_fix: CiAutoFixConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CiAutoFixConfig {
    /// Whether CI auto-fix is enabled. Default: true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum number of fix attempts per PR. Default: 3.
    #[serde(default = "default_ci_fix_max_retries")]
    pub max_retries: u32,
}

impl Default for CiAutoFixConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: default_ci_fix_max_retries(),
        }
    }
}

fn default_ci_fix_max_retries() -> u32 {
    3
}

fn default_test_command() -> String {
    "cargo test".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Whether review dispatch is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Review command template (used when `reviewers` is empty). Variables: {pr_number}, {branch}.
    #[serde(default = "default_review_command")]
    pub command: String,
    /// Multi-reviewer council configuration. If non-empty, overrides `command`.
    #[serde(default)]
    pub reviewers: Vec<ReviewerEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
            reviewers: Vec::new(),
        }
    }
}

fn default_review_command() -> String {
    "gh pr review {pr_number} --comment --body 'Automated review by Maestro'".into()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// Routing rules: label pattern -> model name. First match wins.
    /// Example: { "priority:P0" = "opus", "type:docs" = "haiku" }
    #[serde(default)]
    pub routing: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

fn default_preview_ratio() -> u8 {
    50
}
fn default_activity_log_height() -> u8 {
    25
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
fn default_work_max_retries() -> u32 {
    2
}
fn default_consultation_max_retries() -> u32 {
    0
}
pub(crate) fn default_max_prompt_history() -> usize {
    100
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
fn default_slack_rate_limit() -> u32 {
    10
}
fn default_overflow_threshold_pct() -> u8 {
    70
}
fn default_commit_prompt_pct() -> u8 {
    50
}
fn default_max_fork_depth() -> u8 {
    5
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
mod tests {
    use super::*;

    #[test]
    fn context_overflow_config_defaults_are_correct() {
        let cfg = ContextOverflowConfig::default();
        assert_eq!(cfg.overflow_threshold_pct, 70);
        assert!(cfg.auto_fork);
        assert_eq!(cfg.commit_prompt_pct, 50);
        assert_eq!(cfg.max_fork_depth, 5);
    }

    #[test]
    fn context_overflow_config_deserializes_from_toml() {
        let toml_str = r#"overflow_threshold_pct = 85"#;
        let cfg: ContextOverflowConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.overflow_threshold_pct, 85);
        assert!(cfg.auto_fork); // default untouched
    }

    #[test]
    fn conflict_policy_default_is_warn() {
        let cfg = ConflictConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.policy, ConflictPolicy::Warn);
    }

    #[test]
    fn conflict_policy_deserializes_pause() {
        let toml_str = r#"policy = "pause""#;
        let cfg: ConflictConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.policy, ConflictPolicy::Pause);
        assert!(cfg.enabled); // default untouched
    }

    #[test]
    fn conflict_policy_deserializes_kill() {
        let toml_str = r#"policy = "kill""#;
        let cfg: ConflictConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.policy, ConflictPolicy::Kill);
    }

    #[test]
    fn conflict_policy_label_round_trips() {
        assert_eq!(ConflictPolicy::Warn.label(), "warn");
        assert_eq!(ConflictPolicy::Pause.label(), "pause");
        assert_eq!(ConflictPolicy::Kill.label(), "kill");
    }

    #[test]
    fn config_uses_context_overflow_defaults_when_section_absent() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.sessions.context_overflow.overflow_threshold_pct, 70);
    }

    #[test]
    fn completion_gates_config_defaults_when_section_absent() {
        let cfg: CompletionGatesConfig = toml::from_str("").expect("parse failed");
        assert!(cfg.enabled);
        assert!(cfg.commands.is_empty());
    }

    #[test]
    fn completion_gates_config_deserializes_full_entry() {
        let toml_str = r#"
enabled = true
[[commands]]
name = "fmt"
run = "cargo fmt --check"
required = false
"#;
        let cfg: CompletionGatesConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.commands.len(), 1);
        assert_eq!(cfg.commands[0].name, "fmt");
        assert_eq!(cfg.commands[0].run, "cargo fmt --check");
        assert!(!cfg.commands[0].required);
    }

    #[test]
    fn completion_gate_entry_required_defaults_to_true() {
        let toml_str = r#"
name = "fmt"
run = "cargo fmt --check"
"#;
        let entry: CompletionGateEntry = toml::from_str(toml_str).expect("parse failed");
        assert!(entry.required);
    }

    #[test]
    fn completion_gates_config_multiple_entries_parse_in_order() {
        let toml_str = r#"
[[commands]]
name = "fmt"
run = "cargo fmt --check"
[[commands]]
name = "clippy"
run = "cargo clippy -- -D warnings"
"#;
        let cfg: CompletionGatesConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.commands[0].name, "fmt");
        assert_eq!(cfg.commands[1].name, "clippy");
    }

    #[test]
    fn ci_auto_fix_config_defaults() {
        let cfg = CiAutoFixConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.max_retries, 3);
    }

    #[test]
    fn ci_auto_fix_config_deserializes_from_toml() {
        let toml_str = r#"
enabled = false
max_retries = 5
"#;
        let cfg: CiAutoFixConfig = toml::from_str(toml_str).expect("parse failed");
        assert!(!cfg.enabled);
        assert_eq!(cfg.max_retries, 5);
    }

    #[test]
    fn gates_config_ci_auto_fix_defaults_when_absent() {
        let cfg: GatesConfig = toml::from_str("").expect("parse failed");
        assert!(cfg.ci_auto_fix.enabled);
        assert_eq!(cfg.ci_auto_fix.max_retries, 3);
    }

    #[test]
    fn full_config_load_propagates_ci_auto_fix() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[gates.ci_auto_fix]
max_retries = 7
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.gates.ci_auto_fix.max_retries, 7);
    }

    #[test]
    fn tui_config_defaults_when_section_absent() {
        use crate::tui::theme::ThemePreset;
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.tui.theme.preset, ThemePreset::Dark);
        assert!(cfg.tui.theme.overrides.text_primary.is_none());
    }

    #[test]
    fn tui_config_deserializes_light_preset() {
        use crate::tui::theme::ThemePreset;
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[tui.theme]
preset = "light"
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.tui.theme.preset, ThemePreset::Light);
    }

    // --- Issue #143: FlagsConfig tests ---

    #[test]
    fn flags_config_defaults_to_empty_hashmap() {
        let cfg = FlagsConfig::default();
        assert!(cfg.entries.is_empty());
    }

    #[test]
    fn flags_config_deserializes_from_toml() {
        let toml_str = r#"
ci_auto_fix = true
review_council = false
"#;
        let cfg: FlagsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.entries.get("ci_auto_fix"), Some(&true));
        assert_eq!(cfg.entries.get("review_council"), Some(&false));
    }

    #[test]
    fn flags_config_deserializes_multiple_entries() {
        let toml_str = r#"
continuous_mode = false
ci_auto_fix = true
"#;
        let cfg: FlagsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.entries.get("continuous_mode"), Some(&false));
        assert_eq!(cfg.entries.get("ci_auto_fix"), Some(&true));
        assert_eq!(cfg.entries.len(), 2);
    }

    #[test]
    fn flags_config_handles_unknown_keys() {
        let toml_str = r#"
totally_unknown_flag = true
ci_auto_fix = false
"#;
        let cfg: FlagsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.entries.len(), 2);
        assert_eq!(cfg.entries.get("totally_unknown_flag"), Some(&true));
    }

    #[test]
    fn full_config_flags_defaults_when_section_absent() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert!(cfg.flags.entries.is_empty());
    }

    #[test]
    fn flags_config_non_boolean_value_is_rejected() {
        let toml_str = r#"continuous_mode = "yes""#;
        let result = toml::from_str::<FlagsConfig>(toml_str);
        assert!(
            result.is_err(),
            "non-bool flag value must fail to deserialize"
        );
    }

    #[test]
    fn full_config_flags_parses_when_section_present() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[flags]
ci_auto_fix = true
review_council = false
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.flags.entries.get("ci_auto_fix"), Some(&true));
        assert_eq!(cfg.flags.entries.get("review_council"), Some(&false));
    }

    // --- Issue #275: configurable hollow retry policy ---
    // Group A: merge_legacy_hollow pure function.

    #[test]
    fn merge_both_none_returns_default() {
        let result = merge_legacy_hollow(None, None);
        assert_eq!(result, HollowRetryConfig::default());
    }

    #[test]
    fn merge_legacy_only_maps_to_work_max_retries() {
        let result = merge_legacy_hollow(None, Some(3));
        assert_eq!(result.policy, HollowRetryPolicy::IntentAware);
        assert_eq!(result.work_max_retries, 3);
        assert_eq!(result.consultation_max_retries, 0);
    }

    #[test]
    fn merge_new_section_only_passes_through() {
        let cfg = HollowRetryConfig {
            policy: HollowRetryPolicy::Always,
            work_max_retries: 7,
            consultation_max_retries: 1,
        };
        let result = merge_legacy_hollow(Some(cfg.clone()), None);
        assert_eq!(result, cfg);
    }

    #[test]
    fn merge_both_new_wins() {
        let cfg = HollowRetryConfig {
            policy: HollowRetryPolicy::Always,
            work_max_retries: 7,
            consultation_max_retries: 1,
        };
        let result = merge_legacy_hollow(Some(cfg.clone()), Some(99));
        assert_eq!(result, cfg);
        assert_ne!(result.work_max_retries, 99);
    }

    #[test]
    fn merge_legacy_zero_is_respected() {
        let result = merge_legacy_hollow(None, Some(0));
        assert_eq!(result.work_max_retries, 0);
        assert_eq!(result.consultation_max_retries, 0);
    }

    // Group B: HollowRetryPolicy enum.

    #[test]
    fn hollow_retry_policy_defaults_to_intent_aware() {
        assert_eq!(HollowRetryPolicy::default(), HollowRetryPolicy::IntentAware);
    }

    #[test]
    fn hollow_retry_policy_serializes_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&HollowRetryPolicy::IntentAware).unwrap(),
            r#""intent-aware""#
        );
        assert_eq!(
            serde_json::to_string(&HollowRetryPolicy::Always).unwrap(),
            r#""always""#
        );
        assert_eq!(
            serde_json::to_string(&HollowRetryPolicy::Never).unwrap(),
            r#""never""#
        );
    }

    #[test]
    fn hollow_retry_policy_deserializes_from_kebab_case() {
        let p: HollowRetryPolicy = serde_json::from_str(r#""intent-aware""#).unwrap();
        assert_eq!(p, HollowRetryPolicy::IntentAware);
        let p: HollowRetryPolicy = serde_json::from_str(r#""never""#).unwrap();
        assert_eq!(p, HollowRetryPolicy::Never);
        let p: HollowRetryPolicy = serde_json::from_str(r#""always""#).unwrap();
        assert_eq!(p, HollowRetryPolicy::Always);
    }

    // Group C: HollowRetryConfig defaults + serde.

    #[test]
    fn hollow_retry_config_default_is_intent_aware_with_expected_limits() {
        let cfg = HollowRetryConfig::default();
        assert_eq!(cfg.policy, HollowRetryPolicy::IntentAware);
        assert_eq!(cfg.work_max_retries, 2);
        assert_eq!(cfg.consultation_max_retries, 0);
    }

    #[test]
    fn hollow_retry_config_round_trips_via_serde() {
        let cfg = HollowRetryConfig {
            policy: HollowRetryPolicy::Never,
            work_max_retries: 5,
            consultation_max_retries: 1,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let rt: HollowRetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, cfg);
    }

    // Group D: SessionsConfig TOML parsing.

    #[test]
    fn sessions_config_parses_new_hollow_retry_section() {
        let toml_str = r#"
[hollow_retry]
policy = "never"
work_max_retries = 4
consultation_max_retries = 1
"#;
        let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.hollow_retry.policy, HollowRetryPolicy::Never);
        assert_eq!(cfg.hollow_retry.work_max_retries, 4);
        assert_eq!(cfg.hollow_retry.consultation_max_retries, 1);
    }

    #[test]
    fn sessions_config_parses_legacy_hollow_max_retries() {
        let toml_str = "hollow_max_retries = 3";
        let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.hollow_retry.work_max_retries, 3);
        assert_eq!(cfg.hollow_retry.policy, HollowRetryPolicy::IntentAware);
        assert_eq!(cfg.hollow_retry.consultation_max_retries, 0);
    }

    #[test]
    fn sessions_config_new_section_wins_over_legacy() {
        let toml_str = r#"
hollow_max_retries = 99
[hollow_retry]
work_max_retries = 5
"#;
        let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.hollow_retry.work_max_retries, 5);
    }

    #[test]
    fn sessions_config_empty_sessions_uses_default_hollow_retry() {
        let cfg: SessionsConfig = toml::from_str("").expect("parse failed");
        assert_eq!(cfg.hollow_retry, HollowRetryConfig::default());
    }

    #[test]
    fn sessions_config_round_trips() {
        let original: SessionsConfig = toml::from_str(
            r#"
[hollow_retry]
policy = "never"
work_max_retries = 4
consultation_max_retries = 2
"#,
        )
        .expect("parse failed");
        let serialized = toml::to_string_pretty(&original).expect("serialize failed");
        let rt: SessionsConfig = toml::from_str(&serialized).expect("reparse failed");
        assert_eq!(rt.hollow_retry, original.hollow_retry);
    }

    #[test]
    fn max_prompt_history_defaults_to_100() {
        let cfg: SessionsConfig = toml::from_str("").expect("parse failed");
        assert_eq!(cfg.max_prompt_history, 100);
    }

    #[test]
    fn max_prompt_history_deserializes_from_toml() {
        let toml_str = r#"max_prompt_history = 50"#;
        let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.max_prompt_history, 50);
    }

    #[test]
    fn full_config_hollow_retry_defaults_when_absent() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.sessions.hollow_retry, HollowRetryConfig::default());
    }

    // --- Issue #121: LayoutConfig tests ---

    #[test]
    fn layout_config_defaults() {
        let cfg = LayoutConfig::default();
        assert_eq!(cfg.mode, LayoutMode::Vertical);
        assert_eq!(cfg.density, Density::Default);
        assert_eq!(cfg.preview_ratio, 50);
        assert_eq!(cfg.activity_log_height, 25);
    }

    #[test]
    fn layout_config_deserializes_from_toml() {
        let toml_str = r#"
mode = "horizontal"
density = "compact"
preview_ratio = 60
activity_log_height = 30
"#;
        let cfg: LayoutConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.mode, LayoutMode::Horizontal);
        assert_eq!(cfg.density, Density::Compact);
        assert_eq!(cfg.preview_ratio, 60);
        assert_eq!(cfg.activity_log_height, 30);
    }

    #[test]
    fn layout_config_partial_deserializes() {
        let toml_str = r#"mode = "horizontal""#;
        let cfg: LayoutConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.mode, LayoutMode::Horizontal);
        assert_eq!(cfg.density, Density::Default);
        assert_eq!(cfg.preview_ratio, 50);
    }

    #[test]
    fn layout_config_round_trips() {
        let cfg = LayoutConfig {
            mode: LayoutMode::Horizontal,
            density: Density::Comfortable,
            preview_ratio: 40,
            activity_log_height: 20,
        };
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let reloaded: LayoutConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg, reloaded);
    }

    #[test]
    fn full_config_layout_defaults_when_section_absent() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(cfg.tui.layout.mode, LayoutMode::Vertical);
        assert_eq!(cfg.tui.layout.density, Density::Default);
    }

    // --- Issue #70: Config round-trip (save/load) tests ---

    #[test]
    fn config_save_round_trip_minimal() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let original = Config::load(f.path()).expect("load failed");
        let out = tempfile::NamedTempFile::new().unwrap();
        original.save(out.path()).expect("save failed");
        let reloaded = Config::load(out.path()).expect("reload failed");
        assert_eq!(original, reloaded);
    }

    #[test]
    fn config_save_round_trip_full() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
base_branch = "develop"
[sessions]
max_concurrent = 5
stall_timeout_secs = 600
default_model = "sonnet"
default_mode = "plan"
permission_mode = "default"
allowed_tools = ["Read", "Write"]
max_retries = 3
retry_cooldown_secs = 30
hollow_max_retries = 2
max_prompt_history = 50
[sessions.context_overflow]
overflow_threshold_pct = 85
auto_fork = false
commit_prompt_pct = 60
max_fork_depth = 3
[sessions.conflict]
enabled = false
policy = "kill"
[budget]
per_session_usd = 10.0
total_usd = 100.0
alert_threshold_pct = 90
[github]
issue_filter_labels = ["ready", "approved"]
auto_pr = false
cache_ttl_secs = 600
auto_merge = true
merge_method = "rebase"
[notifications]
desktop = false
slack = true
slack_webhook_url = "https://hooks.slack.com/test"
slack_rate_limit_per_min = 5
[gates]
enabled = false
test_command = "make test"
ci_poll_interval_secs = 60
ci_max_wait_secs = 3600
[gates.ci_auto_fix]
enabled = false
max_retries = 5
[concurrency]
heavy_task_labels = ["gpu", "large"]
heavy_task_limit = 1
[monitoring]
work_tick_interval_secs = 30
[flags]
ci_auto_fix = true
review_council = false
"#
        )
        .unwrap();
        let original = Config::load(f.path()).expect("load failed");
        let out = tempfile::NamedTempFile::new().unwrap();
        original.save(out.path()).expect("save failed");
        let reloaded = Config::load(out.path()).expect("reload failed");
        assert_eq!(original, reloaded);
    }

    #[test]
    fn config_save_writes_all_sections() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let original = Config::load(f.path()).unwrap();
        let out = tempfile::NamedTempFile::new().unwrap();
        original.save(out.path()).unwrap();
        let content = std::fs::read_to_string(out.path()).unwrap();
        assert!(content.contains("[project]"));
        assert!(content.contains("[sessions]"));
        assert!(content.contains("[budget]"));
        assert!(content.contains("max_concurrent"));
        assert!(content.contains("stall_timeout_secs"));
    }

    #[test]
    fn config_save_round_trip_with_completion_gates() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[sessions.completion_gates]
enabled = true
[[sessions.completion_gates.commands]]
name = "fmt"
run = "cargo fmt --check"
required = true
[[sessions.completion_gates.commands]]
name = "clippy"
run = "cargo clippy"
required = false
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let original = Config::load(f.path()).expect("load failed");
        let out = tempfile::NamedTempFile::new().unwrap();
        original.save(out.path()).expect("save failed");
        let reloaded = Config::load(out.path()).expect("reload failed");
        assert_eq!(original, reloaded);
        assert_eq!(reloaded.sessions.completion_gates.commands.len(), 2);
    }

    #[test]
    fn config_partial_eq_detects_difference() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let mut cfg1 = Config::load(f.path()).unwrap();
        let cfg2 = Config::load(f.path()).unwrap();
        assert_eq!(cfg1, cfg2);
        cfg1.sessions.max_concurrent = 999;
        assert_ne!(cfg1, cfg2);
    }

    #[test]
    fn tui_config_deserializes_color_override() {
        use crate::tui::theme::SerializableColor;
        use ratatui::style::Color;
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[tui.theme.overrides]
text_primary = "cyan"
"#
        )
        .unwrap();
        let cfg = Config::load(f.path()).expect("load failed");
        assert_eq!(
            cfg.tui.theme.overrides.text_primary,
            Some(SerializableColor(Color::Cyan))
        );
    }

    // -- TurboQuantConfig --

    #[test]
    fn turboquant_config_defaults_are_correct() {
        let cfg = TurboQuantConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.bit_width, 4);
        assert_eq!(cfg.strategy, QuantStrategy::TurboQuant);
        assert_eq!(cfg.apply_to, ApplyTarget::Both);
        assert!(!cfg.auto_on_overflow);
        assert_eq!(cfg.fork_handoff_budget, 4096);
        assert_eq!(cfg.system_prompt_budget, 2048);
        assert_eq!(cfg.knowledge_budget, 4096);
    }

    #[test]
    fn turboquant_config_fork_handoff_budget_defaults_when_absent() {
        let toml_str = r#"
            enabled = true
            bit_width = 4
        "#;
        let cfg: TurboQuantConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.fork_handoff_budget, 4096);
        assert_eq!(cfg.system_prompt_budget, 2048);
        assert_eq!(cfg.knowledge_budget, 4096);
    }

    #[test]
    fn turboquant_config_new_budgets_deserialize_from_toml() {
        let toml_str = r#"
            enabled = true
            fork_handoff_budget = 8192
            system_prompt_budget = 1024
            knowledge_budget = 16384
        "#;
        let cfg: TurboQuantConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.fork_handoff_budget, 8192);
        assert_eq!(cfg.system_prompt_budget, 1024);
        assert_eq!(cfg.knowledge_budget, 16384);
    }

    #[test]
    fn turboquant_config_absent_section_uses_defaults() {
        let toml_str = r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.turboquant, TurboQuantConfig::default());
    }

    #[test]
    fn turboquant_config_serde_round_trip() {
        let cfg = TurboQuantConfig {
            enabled: true,
            bit_width: 6,
            strategy: QuantStrategy::PolarQuant,
            apply_to: ApplyTarget::Keys,
            auto_on_overflow: true,
            fork_handoff_budget: 8192,
            system_prompt_budget: 1024,
            knowledge_budget: 2048,
        };
        let serialized = toml::to_string(&cfg).unwrap();
        let deserialized: TurboQuantConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(cfg, deserialized);
    }

    #[test]
    fn turboquant_config_deserializes_from_toml() {
        let toml_str = r#"
            enabled = true
            bit_width = 2
            strategy = "qjl"
            apply_to = "values"
            auto_on_overflow = true
        "#;
        let cfg: TurboQuantConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.bit_width, 2);
        assert_eq!(cfg.strategy, QuantStrategy::Qjl);
        assert_eq!(cfg.apply_to, ApplyTarget::Values);
        assert!(cfg.auto_on_overflow);
    }

    #[test]
    fn turboquant_config_on_full_config() {
        let toml_str = r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            [turboquant]
            enabled = true
            bit_width = 3
            strategy = "polarquant"
            apply_to = "both"
            auto_on_overflow = false
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert!(cfg.turboquant.enabled);
        assert_eq!(cfg.turboquant.bit_width, 3);
        assert_eq!(cfg.turboquant.strategy, QuantStrategy::PolarQuant);
    }

    // -- AdaptSettings --

    #[test]
    fn adapt_settings_default_is_ai_naming() {
        let settings = AdaptSettings::default();
        assert_eq!(settings.milestone_naming, MilestoneNaming::Ai);
        assert!(settings.milestone_template.is_none());
    }

    #[test]
    fn adapt_settings_parses_standard_naming() {
        let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 1
default_model = "opus"
default_mode = "orchestrator"
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
[adapt]
milestone_naming = "standard"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.adapt.milestone_naming, MilestoneNaming::Standard);
    }

    #[test]
    fn adapt_settings_parses_custom_naming_with_template() {
        let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 1
default_model = "opus"
default_mode = "orchestrator"
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
[adapt]
milestone_naming = "custom"
milestone_template = "v{n}.0.0 — {title}"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.adapt.milestone_naming, MilestoneNaming::Custom);
        assert_eq!(
            cfg.adapt.milestone_template.as_deref(),
            Some("v{n}.0.0 — {title}")
        );
    }

    #[test]
    fn adapt_settings_defaults_when_section_missing() {
        let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 1
default_model = "opus"
default_mode = "orchestrator"
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.adapt.milestone_naming, MilestoneNaming::Ai);
    }

    // --- Issue #437: LoadedConfig path plumbing ---

    const MINIMAL_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";

    #[test]
    fn find_and_load_in_with_path_returns_resolved_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("maestro.toml");
        std::fs::write(&file_path, MINIMAL_TOML).unwrap();

        let loaded = Config::find_and_load_in_with_path(dir.path()).expect("should find config");

        assert!(
            loaded.path.ends_with("maestro.toml"),
            "path must end with maestro.toml, got {:?}",
            loaded.path
        );
        assert_eq!(loaded.config.project.repo, "owner/repo");
    }

    #[test]
    fn find_and_load_in_with_path_finds_nested_candidate() {
        let dir = tempfile::TempDir::new().unwrap();
        let nested = dir.path().join(".maestro");
        std::fs::create_dir_all(&nested).unwrap();
        let nested_toml = MINIMAL_TOML.replacen("owner/repo", "nested/repo", 1);
        std::fs::write(nested.join("config.toml"), &nested_toml).unwrap();

        let loaded =
            Config::find_and_load_in_with_path(dir.path()).expect("should find nested config");

        assert!(
            loaded.path.ends_with("config.toml"),
            "path must end with config.toml, got {:?}",
            loaded.path
        );
        assert_eq!(loaded.config.project.repo, "nested/repo");
    }

    #[test]
    fn find_and_load_in_with_path_errors_when_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = Config::find_and_load_in_with_path(dir.path());
        assert!(result.is_err(), "should error when no config file present");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("No maestro.toml"),
            "error message should mention 'No maestro.toml', got: {msg}"
        );
    }

    #[test]
    fn find_and_load_shim_still_returns_config_only() {
        // Regression guard: the legacy API must keep returning Result<Config>, not LoadedConfig.
        let _: fn() -> anyhow::Result<Config> = Config::find_and_load;
    }
}
