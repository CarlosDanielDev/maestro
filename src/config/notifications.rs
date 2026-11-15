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

impl NotificationsConfig {
    /// SEC-5 (#1044): an incoming-webhook URL is a bearer credential, and
    /// `maestro.toml` is usually committed. The message never echoes the URL.
    pub fn webhook_leak_warning(&self) -> Option<String> {
        self.slack_webhook_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .map(|_| {
                "notifications.slack_webhook_url is set in maestro.toml: the webhook URL \
                 is a secret. Keep maestro.toml out of version control, or rotate the \
                 webhook if it was ever committed."
                    .to_string()
            })
    }
}

fn default_true() -> bool {
    true
}
fn default_slack_rate_limit() -> u32 {
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_url(url: Option<&str>) -> NotificationsConfig {
        NotificationsConfig {
            desktop: true,
            slack: true,
            slack_webhook_url: url.map(str::to_string),
            slack_rate_limit_per_min: 10,
        }
    }

    #[test]
    fn webhook_in_config_file_warns() {
        let msg = with_url(Some("https://hooks.slack.com/services/T/B/x"))
            .webhook_leak_warning()
            .expect("warning");
        assert!(msg.contains("slack_webhook_url"), "got: {msg}");
        assert!(
            !msg.contains("hooks.slack.com"),
            "warning must not echo the secret"
        );
    }

    #[test]
    fn no_webhook_no_warning() {
        assert!(with_url(None).webhook_leak_warning().is_none());
        assert!(with_url(Some("  ")).webhook_leak_warning().is_none());
    }
}
