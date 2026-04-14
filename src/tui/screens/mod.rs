pub mod adapt;
pub mod hollow_retry;
pub mod home;
#[allow(dead_code)]
pub mod issue_browser;
pub mod milestone;
pub mod pr_review;
pub mod prompt_input;
pub mod queue_confirmation;
pub mod release_notes;
pub mod settings;
pub mod wrap;

pub use hollow_retry::HollowRetryScreen;
pub use home::HomeScreen;
pub use issue_browser::IssueBrowserScreen;
pub use milestone::MilestoneScreen;
pub use prompt_input::PromptInputScreen;
pub use queue_confirmation::QueueConfirmationScreen;
pub use release_notes::ReleaseNotesScreen;
pub use settings::SettingsScreen;

use crate::tui::app::TuiMode;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::theme::Theme;
use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

/// Trait that all interactive screens implement.
pub trait Screen: KeymapProvider {
    /// Handle an input event. Returns a ScreenAction describing what the event loop should do.
    fn handle_input(&mut self, event: &Event, mode: InputMode) -> ScreenAction;

    /// Render the screen into the given area.
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme);

    /// What input mode this screen wants to be in, given its current state.
    /// Returns None to leave the mode unchanged (defer to current global mode).
    fn desired_input_mode(&self) -> Option<InputMode> {
        None
    }
}

/// Sanitize strings from external sources (GitHub API, git) for safe terminal rendering.
/// Strips control characters that could be interpreted as terminal escape sequences.
pub fn sanitize_for_terminal(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

/// Render a keybindings help bar at the bottom of a screen.
pub fn draw_keybinds_bar(f: &mut Frame, area: Rect, bindings: &[(&str, &str)], theme: &Theme) {
    let spans: Vec<Span> = bindings
        .iter()
        .flat_map(|(key, label)| {
            vec![
                Span::styled(
                    format!("[{}]", key),
                    Style::default().fg(theme.accent_success),
                ),
                Span::raw(format!(" {}  ", label)),
            ]
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(spans)), area);
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
    /// Launch a session from a free-form prompt (no issue).
    LaunchPromptSession(PromptSessionConfig),
    /// Request a refresh of dashboard suggestion data.
    RefreshSuggestions,
    /// Launch a unified session for multiple issues (single branch, single PR).
    LaunchUnifiedSession(UnifiedSessionConfig),
    /// Launch a sequential queue execution from confirmed queue.
    LaunchQueue(Vec<SessionConfig>),
    /// Launch a conflict-fix session for a PR with merge conflicts.
    #[allow(dead_code)] // Reason: conflict fix flow — to be wired into PR merge screen
    LaunchConflictFix(ConflictFixConfig),
    /// Retry a hollow-completed session by ID.
    RetryHollow(uuid::Uuid),
    /// Trigger a version check and self-update.
    CheckForUpdate,
    /// Update the live app config (e.g., after Settings save).
    UpdateConfig(Box<crate::config::Config>),
    /// Preview a theme temporarily (reverted on discard).
    PreviewTheme(Option<crate::tui::theme::ThemeConfig>),
    /// Start the adapt pipeline from the wizard screen.
    StartAdaptPipeline(crate::adapt::AdaptConfig),
    /// Fetch PR detail for a specific PR number.
    FetchPrDetail(u64),
    /// Submit a PR review.
    SubmitPrReview {
        pr_number: u64,
        event: crate::github::types::PrReviewEvent,
        body: String,
    },
    /// Quit the application.
    Quit,
}

/// Configuration for launching a conflict-fix session.
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictFixConfig {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub conflicting_files: Vec<String>,
}

/// Configuration for launching a session from a screen action.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    pub issue_number: Option<u64>,
    pub title: String,
    /// Optional custom prompt to append to the issue prompt.
    pub custom_prompt: Option<String>,
}

/// Configuration for launching a unified (multi-issue, single-PR) session.
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedSessionConfig {
    /// All issues to address in a single session: `(number, title)`.
    pub issues: Vec<(u64, String)>,
    /// Optional custom prompt.
    pub custom_prompt: Option<String>,
}

/// Configuration for launching a prompt-based session (no GitHub issue).
#[derive(Debug, Clone, PartialEq)]
pub struct PromptSessionConfig {
    pub prompt: String,
    pub image_paths: Vec<String>,
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    pub fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    pub fn key_event_with_modifiers(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }
}
