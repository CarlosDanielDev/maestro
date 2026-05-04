use serde::{Deserialize, Serialize};

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
pub(super) fn merge_legacy_hollow(
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
    // Default OFF: fresh installs / configs that don't set this explicitly
    // get the safe Claude permission flow (per-tool prompts). Bypass mode
    // is opt-in via the Settings toggle, the `--bypass-review` CLI flag,
    // or by setting `permission_mode = "bypassPermissions"` here.
    "default".into()
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
fn default_true() -> bool {
    true
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
