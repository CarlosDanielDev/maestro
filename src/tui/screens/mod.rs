pub mod home;
pub mod issue_browser;
pub mod milestone;
pub mod prompt_input;

pub use home::HomeScreen;
pub use issue_browser::IssueBrowserScreen;
pub use milestone::MilestoneScreen;
pub use prompt_input::PromptInputScreen;

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
    /// Quit the application.
    Quit,
}

/// Configuration for launching a session from a screen action.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    pub issue_number: Option<u64>,
    pub title: String,
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
