use serde::{Deserialize, Serialize};

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

fn default_true() -> bool {
    true
}
fn default_ci_poll_interval() -> u64 {
    30
}
fn default_ci_max_wait() -> u64 {
    1800
}
