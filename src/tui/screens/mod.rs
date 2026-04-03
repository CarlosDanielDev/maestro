pub mod home;
pub mod issue_browser;
pub mod milestone;

pub use home::HomeScreen;
pub use issue_browser::IssueBrowserScreen;
pub use milestone::MilestoneScreen;

use crate::tui::app::TuiMode;

/// Sanitize strings from external sources (GitHub API, git) for safe terminal rendering.
/// Strips control characters that could be interpreted as terminal escape sequences.
pub fn sanitize_for_terminal(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

/// Action returned by a screen's input handler to drive navigation.
#[derive(Debug, PartialEq)]
pub enum ScreenAction {
    /// No action needed.
    None,
    /// Push a new screen onto the navigation stack.
    Push(TuiMode),
    /// Pop back to the previous screen.
    Pop,
    /// Launch a single session for an issue.
    LaunchSession(SessionConfig),
    /// Launch multiple sessions (e.g., from multi-select or run-all).
    LaunchSessions(Vec<SessionConfig>),
    /// Quit the application.
    Quit,
}

/// Configuration for launching a session from a screen action.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    pub issue_number: Option<u64>,
    pub title: String,
}
