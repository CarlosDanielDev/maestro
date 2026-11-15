use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetConfig {
    #[serde(default = "default_per_session_usd")]
    pub per_session_usd: f64,
    #[serde(default = "default_total_usd")]
    pub total_usd: f64,
    #[serde(default = "default_alert_threshold")]
    pub alert_threshold_pct: u8,
}

fn default_per_session_usd() -> f64 {
    5.0
}
fn default_total_usd() -> f64 {
    50.0
}
fn default_alert_threshold() -> u8 {
    80
}
