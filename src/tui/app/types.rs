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
