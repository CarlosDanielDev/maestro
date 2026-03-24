use chrono::{DateTime, Utc};

/// Severity level for interruptions/notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InterruptLevel {
    /// Informational — logged but no interruption.
    Info,
    /// Warning — budget alerts, retries happening.
    Warning,
    /// Critical — all retries exhausted, CI failure.
    Critical,
    /// Blocker — dependency cycle, all sessions failed, budget exceeded.
    Blocker,
}

impl InterruptLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
            Self::Blocker => "BLOCKER",
        }
    }

    /// Whether this level should trigger a desktop notification.
    pub fn should_notify_desktop(&self) -> bool {
        matches!(self, Self::Critical | Self::Blocker)
    }

    /// Whether this level should show a TUI banner.
    pub fn should_show_banner(&self) -> bool {
        matches!(self, Self::Critical | Self::Blocker)
    }
}

/// A notification/interrupt event.
#[derive(Debug, Clone)]
pub struct Notification {
    pub level: InterruptLevel,
    pub title: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub dismissed: bool,
}

impl Notification {
    pub fn new(
        level: InterruptLevel,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            title: title.into(),
            message: message.into(),
            timestamp: Utc::now(),
            dismissed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_does_not_notify_desktop() {
        assert!(!InterruptLevel::Info.should_notify_desktop());
    }

    #[test]
    fn warning_does_not_notify_desktop() {
        assert!(!InterruptLevel::Warning.should_notify_desktop());
    }

    #[test]
    fn critical_notifies_desktop() {
        assert!(InterruptLevel::Critical.should_notify_desktop());
    }

    #[test]
    fn blocker_notifies_desktop() {
        assert!(InterruptLevel::Blocker.should_notify_desktop());
    }

    #[test]
    fn info_does_not_show_banner() {
        assert!(!InterruptLevel::Info.should_show_banner());
    }

    #[test]
    fn critical_shows_banner() {
        assert!(InterruptLevel::Critical.should_show_banner());
    }

    #[test]
    fn blocker_shows_banner() {
        assert!(InterruptLevel::Blocker.should_show_banner());
    }

    #[test]
    fn levels_are_ordered() {
        assert!(InterruptLevel::Info < InterruptLevel::Warning);
        assert!(InterruptLevel::Warning < InterruptLevel::Critical);
        assert!(InterruptLevel::Critical < InterruptLevel::Blocker);
    }

    #[test]
    fn notification_new_creates_with_correct_fields() {
        let n = Notification::new(InterruptLevel::Critical, "Budget", "90% used");
        assert_eq!(n.level, InterruptLevel::Critical);
        assert_eq!(n.title, "Budget");
        assert_eq!(n.message, "90% used");
        assert!(!n.dismissed);
    }
}
