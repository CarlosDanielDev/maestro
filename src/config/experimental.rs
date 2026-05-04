use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentalConfig {
    #[serde(default)]
    pub azure_devops: bool,
}

impl ExperimentalConfig {
    pub fn is_default(&self) -> bool {
        !self.azure_devops
    }
}
