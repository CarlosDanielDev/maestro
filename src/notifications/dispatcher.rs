use std::process::Command;

use super::slack::{SlackClient, SlackEvent, level_to_slack_event};
use super::types::{InterruptLevel, Notification};

/// Manages notification dispatch — desktop alerts, TUI banners, and Slack webhooks.
pub struct NotificationDispatcher {
    notifications: Vec<Notification>,
    desktop_enabled: bool,
    slack_client: Option<SlackClient>,
}

impl NotificationDispatcher {
    pub fn new(desktop_enabled: bool) -> Self {
        Self {
            notifications: Vec::new(),
            desktop_enabled,
            slack_client: None,
        }
    }

    /// Enable Slack notifications with the given webhook URL and rate limit.
    pub fn with_slack(mut self, webhook_url: String, rate_limit_per_min: u32) -> Self {
        self.slack_client = Some(SlackClient::new(webhook_url, rate_limit_per_min));
        self
    }

    /// Send a notification. Dispatches desktop alerts for Critical/Blocker
    /// and Slack messages for Warning+.
    pub fn notify(&mut self, level: InterruptLevel, title: &str, message: &str) {
        let notification = Notification::new(level, title, message);

        if self.desktop_enabled && level.should_notify_desktop() {
            send_desktop_notification(title, message);
        }

        // Fire-and-forget Slack notification for Warning+
        if self.slack_client.is_some() {
            if let Some(event) = level_to_slack_event(level, title, message) {
                self.send_slack_event(event);
            }
        }

        self.notifications.push(notification);
    }

    /// Send a structured Slack event directly (for richer notifications from app logic).
    pub fn notify_slack(&mut self, event: SlackEvent) {
        self.send_slack_event(event);
    }

    /// Send a test message to verify Slack webhook configuration.
    pub async fn test_slack(&mut self) -> Result<bool, String> {
        match self.slack_client.as_mut() {
            Some(client) => client
                .send_test()
                .await
                .map_err(|e| e.to_string()),
            None => Err("Slack is not configured".into()),
        }
    }

    /// Whether Slack is configured.
    pub fn has_slack(&self) -> bool {
        self.slack_client.is_some()
    }

    fn send_slack_event(&mut self, event: SlackEvent) {
        if let Some(client) = self.slack_client.as_mut() {
            // Spawn a fire-and-forget task. We take the needed data synchronously
            // (rate-limit check) and only spawn if allowed.
            if !client_check_rate_limit(client) {
                tracing::warn!("Slack rate limit reached, dropping notification");
                return;
            }

            // We need to send async but notify() is sync. Use tokio::spawn.
            let payload = event.to_payload();
            let url = client.webhook_url().to_string();
            tokio::spawn(async move {
                let http = reqwest::Client::new();
                match http.post(&url).json(&payload).send().await {
                    Ok(resp) if !resp.status().is_success() => {
                        tracing::error!(
                            "Slack webhook returned HTTP {}",
                            resp.status()
                        );
                    }
                    Err(e) => {
                        tracing::error!("Slack webhook error: {e}");
                    }
                    _ => {
                        tracing::debug!("Slack notification sent successfully");
                    }
                }
            });
        }
    }

    /// Get all active (undismissed) notifications that should show a banner.
    pub fn active_banners(&self) -> Vec<&Notification> {
        self.notifications
            .iter()
            .filter(|n| !n.dismissed && n.level.should_show_banner())
            .collect()
    }

    /// Dismiss the most recent undismissed banner notification.
    pub fn dismiss_latest(&mut self) {
        if let Some(n) = self
            .notifications
            .iter_mut()
            .rev()
            .find(|n| !n.dismissed && n.level.should_show_banner())
        {
            n.dismissed = true;
        }
    }

    /// Get all notifications (for display in detail view).
    pub fn all(&self) -> &[Notification] {
        &self.notifications
    }

    /// Total notification count.
    pub fn count(&self) -> usize {
        self.notifications.len()
    }
}

/// Check rate limit on the client (non-async helper).
fn client_check_rate_limit(client: &mut SlackClient) -> bool {
    // Access the internal rate limiter. We expose this through a method we added.
    client.check_rate_limit_and_record()
}

/// Send a desktop notification using OS-specific tools.
fn send_desktop_notification(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"Maestro: {}\"",
            message.replace('"', "\\\""),
            title.replace('"', "\\\"")
        );
        let _ = Command::new("osascript").args(["-e", &script]).output();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("notify-send")
            .args([&format!("Maestro: {}", title), message])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_dispatcher_has_no_notifications() {
        let d = NotificationDispatcher::new(false);
        assert_eq!(d.count(), 0);
        assert!(d.active_banners().is_empty());
    }

    #[test]
    fn notify_adds_notification() {
        let mut d = NotificationDispatcher::new(false);
        d.notify(InterruptLevel::Warning, "Budget", "80% used");
        assert_eq!(d.count(), 1);
    }

    #[test]
    fn active_banners_only_critical_and_blocker() {
        let mut d = NotificationDispatcher::new(false);
        d.notify(InterruptLevel::Info, "Info", "test");
        d.notify(InterruptLevel::Warning, "Warn", "test");
        d.notify(InterruptLevel::Critical, "Crit", "test");
        d.notify(InterruptLevel::Blocker, "Block", "test");

        let banners = d.active_banners();
        assert_eq!(banners.len(), 2);
    }

    #[test]
    fn dismiss_latest_removes_from_banners() {
        let mut d = NotificationDispatcher::new(false);
        d.notify(InterruptLevel::Critical, "Alert", "something bad");
        assert_eq!(d.active_banners().len(), 1);

        d.dismiss_latest();
        assert!(d.active_banners().is_empty());
    }

    #[test]
    fn dismiss_latest_only_dismisses_one() {
        let mut d = NotificationDispatcher::new(false);
        d.notify(InterruptLevel::Critical, "First", "a");
        d.notify(InterruptLevel::Blocker, "Second", "b");
        assert_eq!(d.active_banners().len(), 2);

        d.dismiss_latest();
        assert_eq!(d.active_banners().len(), 1);
    }

    #[test]
    fn all_returns_all_notifications() {
        let mut d = NotificationDispatcher::new(false);
        d.notify(InterruptLevel::Info, "A", "a");
        d.notify(InterruptLevel::Critical, "B", "b");
        assert_eq!(d.all().len(), 2);
    }

    #[test]
    fn dismissed_notifications_still_in_all() {
        let mut d = NotificationDispatcher::new(false);
        d.notify(InterruptLevel::Critical, "Alert", "test");
        d.dismiss_latest();
        assert_eq!(d.all().len(), 1);
        assert!(d.all()[0].dismissed);
    }

    #[test]
    fn has_slack_returns_false_by_default() {
        let d = NotificationDispatcher::new(false);
        assert!(!d.has_slack());
    }

    #[test]
    fn with_slack_enables_slack() {
        let d = NotificationDispatcher::new(false)
            .with_slack("https://hooks.slack.com/test".into(), 10);
        assert!(d.has_slack());
    }
}
