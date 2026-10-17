use serde::{Deserialize, Serialize};

/// `[quota]`: provider usage windows and headroom routing (issue #1065).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Reserved. No effect in this release: nothing reads this flag yet, so enabling it does nothing.
    /// The follow-up that wires routing must read it before any quota request is sent.
    #[serde(default)]
    pub enabled: bool,
}

impl QuotaConfig {
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}
