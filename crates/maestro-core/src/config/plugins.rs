use serde::{Deserialize, Serialize};

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
