pub mod adapt;
pub mod adapt_follow_up;
pub mod bypass_dispatch;
pub mod bypass_warning;
pub mod ci_error_review;
pub mod gate_output_viewer;
pub mod hollow_retry;
pub mod home;
pub mod interaction;
#[allow(dead_code)]
pub mod issue_browser;
pub mod issue_wizard;
pub mod landing;
pub mod milestone;
pub mod milestone_health;
pub mod milestone_wizard;
pub mod pr_review;
pub mod prd;
pub mod prd_dispatch;
pub mod project_stats;
pub mod prompt_input;
pub mod queue_confirmation;
pub mod release_notes;
pub mod roadmap;
pub mod roadmap_dispatch;
pub mod settings;
pub mod team_wizard;
pub mod wizard_fields;
pub mod wrap;

pub use adapt_follow_up::AdaptFollowUpScreen;
pub use ci_error_review::{CiErrorReviewScreen, CiErrorReviewState, FetchPhase};
pub use hollow_retry::HollowRetryScreen;
pub use home::HomeScreen;
pub use interaction::InteractionScreen;
pub use issue_browser::IssueBrowserScreen;
pub use issue_wizard::IssueWizardScreen;
pub use landing::LandingScreen;
pub use milestone::MilestoneScreen;
pub use milestone_wizard::MilestoneWizardScreen;
pub use project_stats::ProjectStatsScreen;
pub use prompt_input::PromptInputScreen;
pub use queue_confirmation::QueueConfirmationScreen;
pub use release_notes::ReleaseNotesScreen;
pub use settings::SettingsScreen;
pub use team_wizard::TeamWizardScreen;

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

/// Strip markdown code fences (```json, ```, trailing ```) from a Claude
/// response before JSON parsing. Shared by the Milestone Wizard planning
/// flow and the Issue Wizard improve flow (#450).
pub(crate) fn strip_fences(s: &str) -> &str {
    let mut t = s.trim();
    if let Some(stripped) = t.strip_prefix("```json") {
        t = stripped;
    } else if let Some(stripped) = t.strip_prefix("```") {
        t = stripped;
    }
    if let Some(stripped) = t.strip_suffix("```") {
        t = stripped;
    }
    t.trim()
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
    /// Open the read-only diff reviewer on the Interaction screen (#918):
    /// the app computes `git diff merge-base(base, HEAD)` for this worktree
    /// through the `GitOps` seam and hands the text back to the screen.
    OpenInteractionDiff { worktree_path: std::path::PathBuf },
    /// Open a shell at the worktree (the diff reviewer's `o` escape hatch).
    OpenWorktreeShell { worktree_path: std::path::PathBuf },
    /// Dispatch a Team Wizard run. Carries the resolved team name + the user's
    /// input selection plus the wizard's concurrency cap. The dispatcher
    /// re-resolves the `ResolvedTeam` from the wizard's cache, builds a
    /// `Scheduler`, fans out per-level via `LaunchSession`, and routes the
    /// outcome back through `TeamWizardScreen::apply_launch_result`.
    LaunchTeam {
        team_name: String,
        input: crate::orchestration::types::TeamInput,
        max_parallel: usize,
    },
    /// Launch a sequential queue execution from confirmed queue.
    LaunchQueue(Vec<SessionConfig>),
    /// Launch a conflict-fix session for a PR with merge conflicts.
    #[allow(dead_code)] // Reason: conflict fix flow — to be wired into PR merge screen
    LaunchConflictFix(ConflictFixConfig),
    /// Launch a CI fix session from the manual error-review popup (#695).
    LaunchCiFix(CiFixConfig),
    /// Retry a hollow-completed session by ID.
    RetryHollow(uuid::Uuid),
    /// Trigger a version check and self-update.
    CheckForUpdate,
    /// Update the live app config (e.g., after Settings save).
    UpdateConfig(Box<crate::config::Config>),
    /// Re-run project-stack detection from disk and merge results into
    /// the existing `maestro.toml` without overwriting user-customized
    /// keys. Triggered by the "Reset Settings" action on Settings →
    /// Project (#505).
    ResetSettingsFromDetection,
    /// Add or normalize the `[agents]` section in `maestro.toml` using
    /// the same upgrade plan reported by `maestro doctor`.
    NormalizeAgentConfig,
    /// Preview a theme temporarily (reverted on discard).
    PreviewTheme(Option<crate::tui::theme::ThemeConfig>),
    /// Start the adapt pipeline from the wizard screen.
    StartAdaptPipeline(crate::adapt::AdaptConfig),
    /// Fetch PR detail for a specific PR number.
    FetchPrDetail(u64),
    /// Submit a PR review.
    SubmitPrReview {
        pr_number: u64,
        event: crate::provider::types::ReviewEvent,
        body: String,
    },
    /// Open the Issue Wizard with the milestone pre-selected (#326).
    /// `suggested_blocked_by` is the dependency-analysis suggestion the
    /// user may accept or override on the Dependencies step.
    OpenIssueWizardForMilestone {
        milestone: u64,
        suggested_blocked_by: Vec<u64>,
    },
    /// Open the Team Wizard with optional pre-selection. `TuiMode` is `Copy`,
    /// so it can't carry the `String` payload — this carrier variant is
    /// intercepted by the dispatcher and translated into a
    /// `Push(TuiMode::TeamWizard)` plus screen instantiation with the
    /// preselect applied.
    PushTeamWizard {
        mode: team_wizard::TeamWizardMode,
        preselect: Option<team_wizard::TeamLaunchInput>,
    },
    /// Push a one-shot line into the in-app Activity Log. Mirrors a
    /// transient header/inline flash so the message has a permanent home
    /// the user can scroll back to.
    LogActivity {
        tag: String,
        message: String,
        level: crate::tui::activity_log::LogLevel,
    },
    /// Send one interaction turn (Enter or Ctrl+P pushup) for the active
    /// Interaction screen. `prompt` is the resolved text; `issue_number`
    /// keys the `InteractionSession` in the pool (#738).
    SendInteractionTurn { issue_number: u64, prompt: String },
    /// User confirmed `Ctrl+Q`: terminate the interaction session
    /// (UserQuit), keep the worktree, and navigate back to Issues (#738).
    QuitInteraction { issue_number: u64 },
    /// `Enter` on a single issue in the browser: the dispatch resumes an
    /// active interaction session for the issue (skipping the launch dialog)
    /// or, when none exists, opens the launch dialog (#738 re-entry AC).
    ResumeOrLaunchIssue { issue_number: u64 },
}

/// Configuration for launching a conflict-fix session.
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictFixConfig {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub conflicting_files: Vec<String>,
}

/// Configuration for launching a CI fix session from the manual
/// error-review popup (#695).
#[derive(Debug, Clone, PartialEq)]
pub struct CiFixConfig {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    /// The local gate command derived from the failed check name. Embedded
    /// in the agent prompt as a "Before pushing, run `<cmd>`" clause.
    pub local_gate_cmd: Option<String>,
    /// The fetched failure log (or a placeholder when fetch failed).
    pub failure_log: String,
    /// Manual fix attempts start at 1; auto path increments separately.
    pub attempt: u32,
}

/// Configuration for launching a session from a screen action.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    pub issue_number: Option<u64>,
    pub title: String,
    /// Optional custom prompt to append to the issue prompt.
    pub custom_prompt: Option<String>,
    /// Configured agent id to use for the new session. `None` means app default.
    pub agent_id: Option<String>,
    /// Launch-dialog "Produce PR" checkbox: session ends when a PR linked to
    /// the issue is created. Defaults to `true`. Behaviour is wired by later
    /// v0.30.0 milestone issues; this field only carries the choice.
    pub produce_pr: bool,
    /// Launch-dialog "Interaction" checkbox: chat with the agent; session
    /// stays alive. Defaults to `false`. Behaviour wired by later issues.
    pub interaction: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            issue_number: None,
            title: String::new(),
            custom_prompt: None,
            agent_id: None,
            produce_pr: true,
            interaction: false,
        }
    }
}

impl SessionConfig {
    pub fn with_agent_id(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
        self
    }
}

/// Configuration for launching a unified (multi-issue, single-PR) session.
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedSessionConfig {
    /// All issues to address in a single session: `(number, title)`.
    pub issues: Vec<(u64, String)>,
    /// Optional custom prompt.
    pub custom_prompt: Option<String>,
    /// Configured agent id to use for the new session. `None` means app default.
    pub agent_id: Option<String>,
    /// "Produce PR" launch option (#919) — plumbed; semantics wired by later
    /// milestone issues (mirrors `SessionConfig.produce_pr` from #733).
    pub produce_pr: bool,
    /// "Interaction" launch option (#919) — plumbed like `produce_pr`.
    pub interaction: bool,
}

impl UnifiedSessionConfig {
    pub fn with_agent_id(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
        self
    }
}

/// Configuration for launching a prompt-based session (no GitHub issue).
#[derive(Debug, Clone, PartialEq)]
pub struct PromptSessionConfig {
    pub prompt: String,
    pub image_paths: Vec<String>,
    /// Configured agent id to use for the new session. `None` means app default.
    pub agent_id: Option<String>,
    /// "Produce PR" launch option (#919) — plumbed like `SessionConfig`'s.
    pub produce_pr: bool,
    /// "Interaction" launch option (#919) — plumbed like `produce_pr`.
    pub interaction: bool,
}

impl PromptSessionConfig {
    pub fn with_agent_id(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
        self
    }
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
