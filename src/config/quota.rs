use serde::{Deserialize, Serialize};

/// `[quota]`: provider usage windows and headroom routing (issue #1065).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Off by default. When off, maestro makes no quota request and reads no credential file.
    #[serde(default)]
    pub enabled: bool,
}

impl QuotaConfig {
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}
