use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentalConfig {
    /// Retained for backward compatibility with pre-v0.24.0 configs. Azure
    /// DevOps is stable; this flag defaults to true and no longer gates use.
    #[serde(default = "default_azure_devops")]
    pub azure_devops: bool,
}

impl Default for ExperimentalConfig {
    fn default() -> Self {
        Self {
            azure_devops: default_azure_devops(),
        }
    }
}

impl ExperimentalConfig {
    pub fn is_default(&self) -> bool {
        self.azure_devops
    }
}

fn default_azure_devops() -> bool {
    true
}
