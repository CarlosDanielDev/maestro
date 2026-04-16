use crate::adapt::AdaptConfig;
use crate::adapt::types::{AdaptPlan, AdaptReport, MaterializeResult, ProjectProfile};
use crate::github::types::{GhIssue, GhMilestone};
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::types::SessionStatus;
use crate::tui::screens::{PromptSessionConfig, SessionConfig, UnifiedSessionConfig};

/// TUI display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Overview,
    Detail(uuid::Uuid),
    DependencyGraph,
    Fullscreen(uuid::Uuid),
    CostDashboard,
    Dashboard,
    IssueBrowser,
    MilestoneView,
    PromptInput,
    CompletionSummary,
    ContinuousPause,
    #[allow(dead_code)] // Reason: TUI mode — to be wired into queue screen
    QueueConfirmation,
    QueueExecution,
    HollowRetry,
    TokenDashboard,
    #[allow(dead_code)] // Reason: TUI mode — to be wired into sanitize screen
    Sanitize,
    Settings,
    SessionSwitcher,
    AdaptWizard,
    PrReview,
    ReleaseNotes,
    LogViewer(uuid::Uuid),
    ConfirmKill(uuid::Uuid),
    ConfirmExit,
    SessionSummary,
    TurboquantDashboard,
}

impl TuiMode {
    /// Human-readable label for breadcrumb rendering.
    pub fn breadcrumb_label(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Detail(_) => "Detail",
            Self::DependencyGraph => "Dependencies",
            Self::Fullscreen(_) => "Fullscreen",
            Self::CostDashboard => "Cost",
            Self::Dashboard => "Dashboard",
            Self::IssueBrowser => "Issues",
            Self::MilestoneView => "Milestones",
            Self::PromptInput => "Prompt",
            Self::CompletionSummary => "Summary",
            Self::ContinuousPause => "Paused",
            Self::QueueConfirmation => "Queue",
            Self::QueueExecution => "Executing",
            Self::HollowRetry => "Retry",
            Self::TokenDashboard => "Tokens",
            Self::Sanitize => "Sanitize",
            Self::Settings => "Settings",
            Self::SessionSwitcher => "Switcher",
            Self::AdaptWizard => "Adapt",
            Self::PrReview => "PR Review",
            Self::ReleaseNotes => "Release Notes",
            Self::LogViewer(_) => "Logs",
            Self::ConfirmKill(_) => "Confirm Kill",
            Self::ConfirmExit => "Confirm Exit",
            Self::SessionSummary => "Sessions",
            Self::TurboquantDashboard => "TQ Dashboard",
        }
    }
}

/// Navigation stack for consistent back-navigation with a max depth cap.
#[derive(Debug, Clone)]
pub struct NavigationStack {
    stack: Vec<TuiMode>,
    pub max_depth: usize,
}

impl NavigationStack {
    pub const DEFAULT_MAX_DEPTH: usize = 20;

    pub fn new(max_depth: usize) -> Self {
        Self {
            stack: Vec::with_capacity(max_depth),
            max_depth,
        }
    }

    pub fn push(&mut self, mode: TuiMode) {
        if self.stack.len() >= self.max_depth {
            self.stack.remove(0);
        }
        self.stack.push(mode);
    }

    pub fn pop(&mut self) -> Option<TuiMode> {
        self.stack.pop()
    }

    pub fn peek(&self) -> Option<&TuiMode> {
        self.stack.last()
    }

    #[allow(dead_code)] // Reason: used in tests
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    #[allow(dead_code)] // Reason: used in tests
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn breadcrumbs(&self) -> &[TuiMode] {
        &self.stack
    }
}

impl Default for NavigationStack {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_DEPTH)
    }
}

/// Per-session ephemeral UI state (not persisted).
#[derive(Debug, Clone, Default)]
pub struct SessionUiState {
    /// Whether the completion summary popup is shown for this session.
    pub show_summary_popup: bool,
}

/// Payload for suggestion data fetched from GitHub.
pub struct SuggestionDataPayload {
    pub ready_issue_count: usize,
    pub failed_issue_count: usize,
    pub milestones: Vec<(String, u32, u32)>,
    pub open_issue_count: usize,
    pub closed_issue_count: usize,
}

/// Commands queued by synchronous screen action handlers for async processing.
pub enum TuiCommand {
    FetchIssues,
    FetchMilestones,
    FetchSuggestionData,
    LaunchSession(SessionConfig),
    LaunchSessions(Vec<SessionConfig>),
    LaunchPromptSession(PromptSessionConfig),
    LaunchUnifiedSession(UnifiedSessionConfig),
    RunAdaptScan(AdaptConfig),
    RunAdaptAnalyze(AdaptConfig, ProjectProfile),
    RunAdaptPlan(AdaptConfig, ProjectProfile, AdaptReport),
    RunAdaptMaterialize(AdaptPlan, AdaptReport),
    FetchOpenPrs,
    SubmitPrReview {
        pr_number: u64,
        event: crate::github::types::PrReviewEvent,
        body: String,
    },
}

/// Data events delivered from background fetch tasks.
pub enum TuiDataEvent {
    Issues(anyhow::Result<Vec<GhIssue>>),
    Milestones(anyhow::Result<Vec<(GhMilestone, Vec<GhIssue>)>>),
    Issue(anyhow::Result<GhIssue>, Option<String>),
    SuggestionData(SuggestionDataPayload),
    VersionCheckResult(Option<crate::updater::ReleaseInfo>),
    UpgradeResult(Result<String, String>),
    AdaptScanResult(anyhow::Result<Box<ProjectProfile>>),
    AdaptAnalyzeResult(anyhow::Result<AdaptReport>),
    AdaptPlanResult(anyhow::Result<AdaptPlan>),
    AdaptMaterializeResult(anyhow::Result<MaterializeResult>),
    UnifiedIssues(anyhow::Result<Vec<GhIssue>>, Option<String>),
    PullRequests(anyhow::Result<Vec<crate::github::types::GhPullRequest>>),
    PrReviewSubmitted(anyhow::Result<()>),
}

/// A merge conflict suggestion shown in the completion overlay.
#[derive(Debug, Clone)]
pub struct ConflictSuggestion {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub conflicting_files: Vec<String>,
    pub message: String,
}

/// Summary data shown in the post-completion overlay.
#[derive(Debug, Clone)]
pub struct CompletionSummaryData {
    pub session_count: usize,
    pub total_cost_usd: f64,
    pub sessions: Vec<CompletionSessionLine>,
    pub suggestions: Vec<ConflictSuggestion>,
    pub selected_suggestion: usize,
}

/// Per-gate failure detail shown in the completion summary overlay.
#[derive(Debug, Clone)]
pub struct GateFailureInfo {
    pub gate: String,
    pub message: String,
}

/// Per-session line in the completion summary.
#[derive(Debug, Clone)]
pub struct CompletionSessionLine {
    pub session_id: uuid::Uuid,
    pub label: String,
    pub status: SessionStatus,
    pub cost_usd: f64,
    pub elapsed: String,
    pub pr_link: String,
    pub error_summary: String,
    pub gate_failures: Vec<GateFailureInfo>,
    pub issue_number: Option<u64>,
    pub model: String,
}

impl CompletionSummaryData {
    pub fn has_needs_review(&self) -> bool {
        self.sessions
            .iter()
            .any(|s| s.status == SessionStatus::NeedsReview)
    }

    pub fn has_conflict_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
    }
}

/// State for the on-demand session summary page ([S] keybinding).
#[derive(Debug, Clone, Default)]
pub struct SessionSummaryState {
    pub scroll_offset: u16,
    pub selected_index: usize,
    pub expanded: std::collections::HashSet<uuid::Uuid>,
}

impl SessionSummaryState {
    pub fn toggle_expand(&mut self, id: uuid::Uuid) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }
}

pub(crate) struct PendingHook {
    pub hook: HookPoint,
    pub ctx: HookContext,
}

pub(crate) struct PendingIssueCompletion {
    pub issue_number: u64,
    /// Additional issue numbers for unified PR sessions.
    pub issue_numbers: Vec<u64>,
    pub success: bool,
    pub cost_usd: f64,
    pub files_touched: Vec<String>,
    pub worktree_branch: Option<String>,
    pub worktree_path: Option<std::path::PathBuf>,
    pub is_ci_fix: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── NavigationStack ────────────────────────────────────────────────

    #[test]
    fn push_and_pop_returns_last() {
        let id = uuid::Uuid::new_v4();
        let mut stack = NavigationStack::default();
        stack.push(TuiMode::Overview);
        stack.push(TuiMode::Detail(id));
        assert_eq!(stack.pop(), Some(TuiMode::Detail(id)));
    }

    #[test]
    fn pop_empty_returns_none() {
        let mut stack = NavigationStack::default();
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn max_depth_drops_oldest_when_exceeded() {
        let mut stack = NavigationStack::default();
        stack.push(TuiMode::Dashboard);
        for _ in 0..20 {
            stack.push(TuiMode::Overview);
        }
        assert_eq!(stack.depth(), 20);
        assert!(stack.breadcrumbs().iter().all(|m| *m == TuiMode::Overview));
    }

    #[test]
    fn clear_empties_stack() {
        let mut stack = NavigationStack::default();
        stack.push(TuiMode::Overview);
        stack.push(TuiMode::IssueBrowser);
        stack.push(TuiMode::Settings);
        stack.clear();
        assert!(stack.is_empty());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn breadcrumbs_returns_ordered_slice_oldest_first() {
        let id = uuid::Uuid::new_v4();
        let mut stack = NavigationStack::default();
        stack.push(TuiMode::Overview);
        stack.push(TuiMode::Detail(id));
        stack.push(TuiMode::Settings);
        let crumbs = stack.breadcrumbs();
        assert_eq!(crumbs[0], TuiMode::Overview);
        assert_eq!(crumbs[1], TuiMode::Detail(id));
        assert_eq!(crumbs[2], TuiMode::Settings);
    }

    #[test]
    fn depth_tracks_size_after_push_and_pop() {
        let mut stack = NavigationStack::default();
        stack.push(TuiMode::Overview);
        stack.push(TuiMode::Dashboard);
        stack.push(TuiMode::Settings);
        assert_eq!(stack.depth(), 3);
        let _ = stack.pop();
        assert_eq!(stack.depth(), 2);
    }

    #[test]
    fn default_has_20_max_depth() {
        let stack = NavigationStack::default();
        assert_eq!(stack.max_depth, 20);
    }

    #[test]
    fn peek_returns_last_without_removing() {
        let id = uuid::Uuid::new_v4();
        let mut stack = NavigationStack::default();
        stack.push(TuiMode::Overview);
        stack.push(TuiMode::Detail(id));
        assert_eq!(stack.peek(), Some(&TuiMode::Detail(id)));
        assert_eq!(stack.peek(), Some(&TuiMode::Detail(id)));
        assert_eq!(stack.depth(), 2);
    }

    // ── TuiMode::breadcrumb_label ───────────────────────────────────────

    #[test]
    fn all_variants_return_non_empty_breadcrumb_label() {
        let id = uuid::Uuid::new_v4();
        let variants: &[TuiMode] = &[
            TuiMode::Overview,
            TuiMode::Dashboard,
            TuiMode::IssueBrowser,
            TuiMode::MilestoneView,
            TuiMode::DependencyGraph,
            TuiMode::CostDashboard,
            TuiMode::TokenDashboard,
            TuiMode::TurboquantDashboard,
            TuiMode::Settings,
            TuiMode::PromptInput,
            TuiMode::SessionSwitcher,
            TuiMode::AdaptWizard,
            TuiMode::PrReview,
            TuiMode::ReleaseNotes,
            TuiMode::CompletionSummary,
            TuiMode::ContinuousPause,
            TuiMode::QueueConfirmation,
            TuiMode::QueueExecution,
            TuiMode::HollowRetry,
            TuiMode::Sanitize,
            TuiMode::SessionSummary,
            TuiMode::ConfirmExit,
            TuiMode::Detail(id),
            TuiMode::Fullscreen(id),
            TuiMode::LogViewer(id),
            TuiMode::ConfirmKill(id),
        ];
        for mode in variants {
            let label = mode.breadcrumb_label();
            assert!(
                !label.is_empty(),
                "breadcrumb_label() returned empty string for {:?}",
                mode
            );
        }
    }
}
