use serde::{Deserialize, Serialize};

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
