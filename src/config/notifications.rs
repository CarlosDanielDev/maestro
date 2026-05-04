use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default)]
    pub slack: bool,
    /// Slack webhook URL for sending notifications.
    #[serde(default)]
    pub slack_webhook_url: Option<String>,
    /// Maximum Slack messages per minute (rate limiting). Default: 10.
    #[serde(default = "default_slack_rate_limit")]
    pub slack_rate_limit_per_min: u32,
}

fn default_true() -> bool {
    true
}
fn default_slack_rate_limit() -> u32 {
    10
}
