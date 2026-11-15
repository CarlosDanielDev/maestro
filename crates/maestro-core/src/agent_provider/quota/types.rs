use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One rate-limit window as the provider reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub utilization_pct: f64,
    pub resets_at: DateTime<Utc>,
}

/// Windows for one provider. A `None` window was not reported. It is never 0%.
/// `stale` is true when the snapshot is older than the TTL or polling is paused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderWindows {
    pub five_hour: Option<QuotaWindow>,
    pub seven_day: Option<QuotaWindow>,
    pub seven_day_sonnet: Option<QuotaWindow>,
    pub stale: bool,
}

/// Network failure. Holds only a message. Never a URL with a token or a header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
