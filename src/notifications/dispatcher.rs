use std::process::Command;

use super::types::{InterruptLevel, Notification};

/// Manages notification dispatch — desktop alerts and TUI banners.
pub struct NotificationDispatcher {
    notifications: Vec<Notification>,
    desktop_enabled: bool,
}

impl NotificationDispatcher {
    pub fn new(desktop_enabled: bool) -> Self {
        Self {
            notifications: Vec::new(),
            desktop_enabled,
        }
    }

    /// Send a notification. Dispatches desktop alerts for Critical/Blocker.
    pub fn notify(&mut self, level: InterruptLevel, title: &str, message: &str) {
        let notification = Notification::new(level, title, message);

        if self.desktop_enabled && level.should_notify_desktop() {
            send_desktop_notification(title, message);
        }

        self.notifications.push(notification);
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
}
