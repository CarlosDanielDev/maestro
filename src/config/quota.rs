use serde::{Deserialize, Serialize};

/// `[quota]`: provider usage windows and headroom routing (issue #1065).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Opt-in provider quota checks (#1065). When `true`, maestro polls each
    /// provider's usage windows every 60s in the background, routes a new
    /// session with no explicit provider to the one with the most headroom,
    /// shows a badge per provider on the token dashboard, and alerts once when
    /// a window reaches 97% (re-arms below 90%). When `false`, no credential is
    /// read and no request is sent.
    #[serde(default)]
    pub enabled: bool,
}

impl QuotaConfig {
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}
