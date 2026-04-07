use crate::budget::{BudgetAction, BudgetCheck, BudgetEnforcer};
use crate::config::Config;
use crate::config::ConflictPolicy;
use crate::continuous::ContinuousModeState;
use crate::gates::runner::{self, GateCheck, GateRunner};
use crate::gates::types::{CompletionGate, GateResult};
use crate::git::GitOps;
use crate::github::ci::{CiCheck, CiChecker, CiStatus, PendingPrCheck};
use crate::github::client::GitHubClient;
use crate::github::labels::LabelManager;
use crate::github::pr::PrCreator;
use crate::github::types::{GhIssue, GhMilestone};
use crate::models::ModelRouter;
use crate::notifications::dispatcher::NotificationDispatcher;
use crate::notifications::slack::SlackEvent;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::plugins::runner::PluginRunner;
use crate::prompts::PromptBuilder;
use crate::session::context_monitor::{ContextMonitor, ProductionContextMonitor};
use crate::session::fork::{ForkPolicy, ForkReason, ForkResult, SessionForker};
use crate::session::health::{HealthCheck, HealthMonitor};
use crate::session::logger::SessionLogger;
use crate::session::manager::SessionEvent;
use crate::session::pool::SessionPool;
use crate::session::retry::RetryPolicy;
use crate::session::types::{Session, SessionStatus, StreamEvent};
use crate::session::worktree::WorktreeManager;
use crate::state::file_claims::{ClaimResult, FILE_CONFLICT_SENTINEL};
use crate::state::progress::ProgressTracker;
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use crate::tui::activity_log::{ActivityLog, LogLevel};
use crate::tui::panels::PanelView;
use crate::tui::screens::milestone::MilestoneEntry;
use crate::tui::screens::{PromptSessionConfig, SessionConfig};
use crate::tui::theme::Theme;
use crate::work::assigner::WorkAssigner;
use chrono::Utc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// TUI display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    /// Default overview with side-by-side panels.
    Overview,
    /// Detail view for a specific session (by index in the session list).
    Detail(usize),
    /// Dependency graph visualization.
    DependencyGraph,
    /// Full-screen agent view (expanded single agent output).
    Fullscreen(usize),
    /// Cost dashboard view.
    CostDashboard,
    /// Interactive home/dashboard screen.
    Dashboard,
    /// Interactive issue browser.
    IssueBrowser,
    /// Milestone overview with progress tracking.
    MilestoneView,
    /// Prompt input screen for composing free-form prompts with image attachments.
    PromptInput,
    /// Post-completion summary overlay.
    CompletionSummary,
    /// Continuous mode failure pause overlay (skip/retry/stop).
    ContinuousPause,
}

/// Payload for suggestion data fetched from GitHub.
pub struct SuggestionDataPayload {
    pub ready_issue_count: usize,
    pub failed_issue_count: usize,
    pub milestones: Vec<(String, u32, u32)>,
}

/// Commands queued by synchronous screen action handlers for async processing.
pub enum TuiCommand {
    FetchIssues,
    FetchMilestones,
    FetchSuggestionData,
    LaunchSession(SessionConfig),
    LaunchSessions(Vec<SessionConfig>),
    LaunchPromptSession(PromptSessionConfig),
}

/// Data events delivered from background fetch tasks.
pub enum TuiDataEvent {
    Issues(anyhow::Result<Vec<GhIssue>>),
    Milestones(anyhow::Result<Vec<(GhMilestone, Vec<GhIssue>)>>),
    /// Single issue for session launch — ready to create session.
    /// The optional String carries a custom prompt to append.
    Issue(anyhow::Result<GhIssue>, Option<String>),
    /// Suggestion data for the home screen.
    SuggestionData(SuggestionDataPayload),
    /// Version check result from background task.
    VersionCheckResult(Option<crate::updater::ReleaseInfo>),
    /// Binary upgrade result (Ok = backup_path, Err = error message).
    UpgradeResult(Result<String, String>),
}

/// Summary data shown in the post-completion overlay.
#[derive(Debug, Clone)]
pub struct CompletionSummaryData {
    pub session_count: usize,
    pub total_cost_usd: f64,
    pub sessions: Vec<CompletionSessionLine>,
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
    /// Returns true if any session ended in NeedsReview (gate failures present).
    pub fn has_needs_review(&self) -> bool {
        self.sessions
            .iter()
            .any(|s| s.status == SessionStatus::NeedsReview)
    }
}

struct PendingHook {
    hook: HookPoint,
    ctx: HookContext,
}

struct PendingIssueCompletion {
    issue_number: u64,
    success: bool,
    cost_usd: f64,
    files_touched: Vec<String>,
    worktree_branch: Option<String>,
    worktree_path: Option<std::path::PathBuf>,
    is_ci_fix: bool,
}

pub struct App {
    pub pool: SessionPool,
    pub activity_log: ActivityLog,
    pub panel_view: PanelView,
    pub state: MaestroState,
    pub store: StateStore,
    pub running: bool,
    pub total_cost: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    /// Work assigner for GitHub issue-based runs. None for prompt-only runs.
    pub work_assigner: Option<WorkAssigner>,
    /// GitHub client for label updates and PR creation.
    pub github_client: Option<Box<dyn GitHubClient>>,
    /// Config reference for base_branch, auto_pr, etc.
    pub config: Option<Config>,
    /// Pending issue completions to process in the next async check_completions tick.
    pending_issue_completions: Vec<PendingIssueCompletion>,
    /// Pending plugin hooks to fire in the next async tick.
    pending_hooks: Vec<PendingHook>,
    /// Health monitor for stall detection.
    pub health_monitor: Box<dyn HealthCheck>,
    /// Budget enforcer for cost limits.
    pub budget_enforcer: Option<BudgetEnforcer>,
    /// Model router for task-based model selection.
    pub model_router: Option<ModelRouter>,
    /// Progress tracker for per-session phase tracking.
    pub progress_tracker: ProgressTracker,
    /// Notification dispatcher for interruption system.
    pub notifications: NotificationDispatcher,
    /// Current TUI display mode.
    pub tui_mode: TuiMode,
    /// Session transcript logger.
    pub session_logger: SessionLogger,
    /// PRs awaiting CI completion.
    pub pending_pr_checks: Vec<PendingPrCheck>,
    /// Last time CI status was polled.
    last_ci_poll: Instant,
    /// Last time work assigner was ticked.
    last_work_tick: Instant,
    /// Plugin runner for hook-based plugin execution.
    pub plugin_runner: Option<PluginRunner>,
    /// Whether the help overlay is visible.
    pub show_help: bool,
    /// Scroll offset for the help overlay.
    pub help_scroll: u16,
    /// Context overflow monitor.
    pub context_monitor: Box<dyn ContextMonitor>,
    /// Fork policy for auto-fork decisions.
    pub fork_policy: Option<ForkPolicy>,
    /// Home screen state (for Dashboard mode).
    pub home_screen: Option<crate::tui::screens::HomeScreen>,
    /// Issue browser screen state (for IssueBrowser mode).
    pub issue_browser_screen: Option<crate::tui::screens::IssueBrowserScreen>,
    /// Milestone screen state (for MilestoneView mode).
    pub milestone_screen: Option<crate::tui::screens::MilestoneScreen>,
    /// Prompt input screen state (for PromptInput mode).
    pub prompt_input_screen: Option<crate::tui::screens::PromptInputScreen>,
    /// Pending TUI commands to process in the next event loop tick.
    pub pending_commands: Vec<TuiCommand>,
    /// Sessions ready to be launched (created from background IssueFetched events).
    pub pending_session_launches: Vec<Session>,
    /// Sender for background data fetch results.
    pub data_tx: mpsc::UnboundedSender<TuiDataEvent>,
    /// Receiver for background data fetch results.
    pub data_rx: mpsc::UnboundedReceiver<TuiDataEvent>,
    /// Active theme for TUI rendering.
    pub theme: Theme,
    /// Whether to exit after all sessions complete (--once flag for CI).
    pub once_mode: bool,
    /// Data for the completion summary overlay.
    pub completion_summary: Option<CompletionSummaryData>,
    /// Continuous mode state (Some when --continuous flag is active).
    pub continuous_mode: Option<ContinuousModeState>,
    /// Current state of the self-upgrade flow.
    pub upgrade_state: crate::updater::UpgradeState,
    /// Tick counter for spinner animation (incremented each TUI draw cycle).
    pub spinner_tick: usize,
}

impl App {
    pub fn new(
        store: StateStore,
        max_concurrent: usize,
        worktree_mgr: Box<dyn WorktreeManager + Send>,
        permission_mode: String,
        allowed_tools: Vec<String>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let state = store.load().unwrap_or_default();
        let mut pool = SessionPool::new(max_concurrent, worktree_mgr, event_tx.clone());
        pool.set_permission_mode(permission_mode);
        pool.set_allowed_tools(allowed_tools);
        Self {
            pool,
            activity_log: ActivityLog::new(500),
            panel_view: PanelView::new(),
            state,
            store,
            running: true,
            total_cost: 0.0,
            start_time: Utc::now(),
            event_rx,
            work_assigner: None,
            github_client: None,
            config: None,
            pending_issue_completions: Vec::new(),
            pending_hooks: Vec::new(),
            health_monitor: Box::new(HealthMonitor::new()),
            budget_enforcer: None,
            model_router: None,
            progress_tracker: ProgressTracker::new(),
            notifications: NotificationDispatcher::new(false),
            tui_mode: TuiMode::Overview,
            session_logger: SessionLogger::new(SessionLogger::default_dir()),
            pending_pr_checks: Vec::new(),
            last_ci_poll: Instant::now(),
            last_work_tick: Instant::now(),
            plugin_runner: None,
            show_help: false,
            help_scroll: 0,

            context_monitor: Box::new(ProductionContextMonitor::new()),
            fork_policy: None,
            home_screen: None,
            issue_browser_screen: None,
            milestone_screen: None,
            prompt_input_screen: None,
            pending_commands: Vec::new(),
            pending_session_launches: Vec::new(),
            data_tx,
            data_rx,
            theme: Theme::default(),
            once_mode: false,
            completion_summary: None,
            continuous_mode: None,
            upgrade_state: crate::updater::UpgradeState::Hidden,
            spinner_tick: 0,
        }
    }

    /// Configure the app with a loaded Config, setting up fork policy and other config-dependent fields.
    pub fn configure(&mut self, config: Config) {
        self.fork_policy = Some(ForkPolicy::new(
            config.sessions.context_overflow.max_fork_depth,
        ));
        // Resolve guardrail prompt: custom from config or auto-detected default
        let guardrail = crate::prompts::resolve_guardrail(
            config.sessions.guardrail_prompt.as_deref(),
            &std::path::PathBuf::from("."),
        );
        self.pool.set_guardrail_prompt(guardrail);
        let mut theme = Theme::from_config(&config.tui.theme);
        theme.apply_capability(crate::tui::theme::ColorCapability::detect());
        self.theme = theme;
        self.config = Some(config);
    }

    /// Add a session and try to promote/spawn it.
    pub async fn add_session(&mut self, session: Session) -> anyhow::Result<()> {
        let label = session_label(&session);
        self.activity_log
            .push_simple(label.clone(), "Enqueuing session...".into(), LogLevel::Info);

        self.pool.enqueue(session);

        // Try to promote and spawn
        let promoted_ids = self.pool.try_promote();
        let tx = self.pool.event_tx();
        for id in promoted_ids {
            if let Some(managed) = self.pool.get_active_mut(id) {
                let session_label = session_label(&managed.session);
                self.activity_log.push_simple(
                    session_label.clone(),
                    "Spawning session...".into(),
                    LogLevel::Info,
                );
                if let Err(e) = managed.spawn(tx.clone()).await {
                    self.activity_log.push_simple(
                        session_label,
                        format!("Spawn failed: {}", e),
                        LogLevel::Error,
                    );
                } else {
                    self.activity_log.push_simple(
                        session_label,
                        "Session started".into(),
                        LogLevel::Info,
                    );
                    // Fire session_started plugin hook
                    let ctx = HookContext::new().with_session(
                        &managed.session.id.to_string(),
                        managed.session.issue_number,
                    );
                    self.fire_plugin_hook(HookPoint::SessionStarted, ctx).await;
                }
            }
        }

        self.sync_state();
        Ok(())
    }

    /// Process a stream event from a session.
    pub fn handle_session_event(&mut self, evt: SessionEvent) {
        let session_id = evt.session_id;

        // Log event to session transcript
        let _ = self.session_logger.log_event(session_id, &evt.event);

        // Record activity for stall detection
        self.health_monitor.record_activity(session_id);

        // File claim processing for mutating tools
        if let StreamEvent::ToolUse {
            ref tool,
            file_path: Some(ref path),
            ..
        } = evt.event
            && matches!(tool.as_str(), "Write" | "Edit")
        {
            let result = self.pool.file_claims.claim(path, session_id);
            if let ClaimResult::Conflict { owner } = result {
                let label = format!("S-{}", &session_id.to_string()[..8]);
                let owner_short = &owner.to_string()[..8];

                // Record conflict in history
                self.pool
                    .file_claims
                    .record_conflict(path, owner, session_id);

                self.activity_log.push_simple(
                    label,
                    format!("CONFLICT: {} claimed by S-{}", path, owner_short),
                    LogLevel::Error,
                );

                // Emit real-time TUI notification
                self.notifications.notify(
                    crate::notifications::types::InterruptLevel::Critical,
                    "File Conflict",
                    &format!(
                        "S-{} tried to write {} (owned by S-{})",
                        &session_id.to_string()[..8],
                        path,
                        owner_short
                    ),
                );
                // Send structured Slack notification for file conflict
                self.notifications.notify_slack(SlackEvent::FileConflict {
                    file_path: path.to_string(),
                    sessions: vec![session_id.to_string(), owner.to_string()],
                });

                // Enforce conflict policy
                let policy = self
                    .config
                    .as_ref()
                    .map(|c| c.sessions.conflict.policy)
                    .unwrap_or(ConflictPolicy::Warn);

                match policy {
                    ConflictPolicy::Warn => {
                        // Already logged and notified above
                    }
                    ConflictPolicy::Pause => {
                        #[cfg(unix)]
                        if let Some(managed) = self.pool.get_active_mut(session_id) {
                            let _ = managed.pause();
                            managed.session.status = SessionStatus::Paused;
                            managed
                                .session
                                .log_activity(format!("Paused due to conflict on {}", path));
                            self.activity_log.push_simple(
                                format!("S-{}", &session_id.to_string()[..8]),
                                format!("Session paused (conflict policy) on {}", path),
                                LogLevel::Warn,
                            );
                        }
                    }
                    ConflictPolicy::Kill => {
                        if let Some(managed) = self.pool.get_active_mut(session_id) {
                            managed.session.status = SessionStatus::Killed;
                            managed.session.finished_at = Some(Utc::now());
                            managed
                                .session
                                .log_activity(format!("Killed due to conflict on {}", path));
                            self.activity_log.push_simple(
                                format!("S-{}", &session_id.to_string()[..8]),
                                format!("Session killed (conflict policy) on {}", path),
                                LogLevel::Error,
                            );
                        }
                    }
                }

                // Queue file_conflict hook
                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::FileConflict,
                    ctx: HookContext::new()
                        .with_session(&session_id.to_string(), None)
                        .with_var("MAESTRO_CONFLICT_FILE", path)
                        .with_var("MAESTRO_CONFLICT_OWNER", &owner.to_string())
                        .with_var("MAESTRO_CONFLICT_POLICY", policy.label()),
                });
            }
        }

        // Sentinel detection
        if let StreamEvent::AssistantMessage { ref text } = evt.event
            && text.contains(FILE_CONFLICT_SENTINEL)
        {
            let label = format!("S-{}", &session_id.to_string()[..8]);
            self.activity_log.push_simple(
                label,
                "FILE_CONFLICT sentinel detected!".into(),
                LogLevel::Error,
            );
        }

        // Delegate event handling to pool's managed session
        if let Some(managed) = self.pool.get_active_mut(session_id) {
            managed.handle_event(&evt.event);
            let label = session_label(&managed.session);

            match &evt.event {
                StreamEvent::ToolUse {
                    tool,
                    file_path,
                    command_preview,
                    ..
                } => {
                    let detail = match (
                        tool.as_str(),
                        file_path.as_deref(),
                        command_preview.as_deref(),
                    ) {
                        ("Bash", _, Some(cmd)) => format!("$ {}", cmd),
                        (t, Some(path), _) => format!("{}: {}", t, path),
                        (t, None, _) => format!("Using {}", t),
                    };
                    self.activity_log.push_simple(label, detail, LogLevel::Tool);
                    // Track progress phase
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_tool_use(tool, file_path.as_deref());
                }
                StreamEvent::AssistantMessage { text } => {
                    // Do NOT push every text chunk to global activity log (anti-flood)
                    // Track progress phase from message content
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_message(text);
                }
                StreamEvent::Thinking { .. } => {
                    // Thinking is tracked per-session via current_activity
                    // No global activity log entry needed
                }
                StreamEvent::Completed { cost_usd } => {
                    self.activity_log.push_simple(
                        label,
                        format!("Completed (${:.2})", cost_usd),
                        LogLevel::Info,
                    );
                    // Send Slack notification for session completion
                    self.notifications
                        .notify_slack(SlackEvent::SessionCompleted {
                            session_id: managed.session.id.to_string(),
                            issue_number: managed.session.issue_number,
                            cost_usd: *cost_usd,
                        });
                    // Queue session_completed plugin hook
                    self.pending_hooks.push(PendingHook {
                        hook: HookPoint::SessionCompleted,
                        ctx: HookContext::new()
                            .with_session(
                                &managed.session.id.to_string(),
                                managed.session.issue_number,
                            )
                            .with_cost(*cost_usd)
                            .with_files(&managed.session.files_touched),
                    });
                    // Queue issue completion for async processing
                    if let Some(issue_num) = managed.session.issue_number {
                        self.pending_issue_completions.push(PendingIssueCompletion {
                            issue_number: issue_num,
                            success: true,
                            cost_usd: *cost_usd,
                            files_touched: managed.session.files_touched.clone(),
                            worktree_branch: managed.branch_name.clone(),
                            worktree_path: managed.worktree_path.clone(),
                            is_ci_fix: managed.session.ci_fix_context.is_some(),
                        });
                    }
                }
                StreamEvent::Error { message } => {
                    self.activity_log.push_simple(
                        label,
                        format!("ERROR: {}", message),
                        LogLevel::Error,
                    );
                    // Send Slack notification for session error
                    self.notifications.notify_slack(SlackEvent::SessionErrored {
                        session_id: managed.session.id.to_string(),
                        issue_number: managed.session.issue_number,
                        error: message.clone(),
                    });
                    // Queue issue failure for async processing
                    if let Some(issue_num) = managed.session.issue_number {
                        self.pending_issue_completions.push(PendingIssueCompletion {
                            issue_number: issue_num,
                            success: false,
                            cost_usd: managed.session.cost_usd,
                            files_touched: managed.session.files_touched.clone(),
                            worktree_branch: managed.branch_name.clone(),
                            worktree_path: managed.worktree_path.clone(),
                            is_ci_fix: managed.session.ci_fix_context.is_some(),
                        });
                    }
                }
                StreamEvent::ContextUpdate { context_pct } => {
                    self.context_monitor
                        .record_context(session_id, *context_pct);
                }
                _ => {}
            }
        }

        // Context overflow checks (only on context updates to avoid hot-path waste)
        if matches!(evt.event, StreamEvent::ContextUpdate { .. }) {
            self.check_context_overflow(session_id);
        }

        // Budget enforcement on cost updates
        self.check_budget(session_id);

        self.sync_state();
    }

    /// Check context overflow for a session and trigger auto-fork if needed.
    fn check_context_overflow(&mut self, session_id: uuid::Uuid) {
        let Some(ref config) = self.config else {
            return;
        };
        let ctx_cfg = &config.sessions.context_overflow;

        // Check commit prompt threshold
        if self
            .context_monitor
            .check_commit_prompt(session_id, ctx_cfg.commit_prompt_ratio())
        {
            self.context_monitor.mark_commit_prompted(session_id);
            let label = self
                .pool
                .get_active_mut(session_id)
                .map(|m| session_label(&m.session))
                .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
            self.activity_log.push_simple(
                label,
                format!(
                    "Context at {}%+ — consider committing work",
                    ctx_cfg.commit_prompt_pct
                ),
                LogLevel::Warn,
            );
        }

        // Check overflow threshold
        if !ctx_cfg.auto_fork {
            return;
        }
        let overflow = self
            .context_monitor
            .check_overflow(session_id, ctx_cfg.overflow_ratio());
        let Some(overflow) = overflow else {
            return;
        };
        let Some(ref fork_policy) = self.fork_policy else {
            return;
        };

        // Get parent session info
        let Some(managed) = self.pool.get_active_mut(session_id) else {
            return;
        };
        let parent_session = managed.session.clone();
        let progress = self.progress_tracker.get(&session_id);

        let fork_result = fork_policy.prepare_fork(
            &parent_session,
            progress,
            ForkReason::ContextOverflow {
                context_pct: overflow.context_pct,
            },
        );

        match fork_result {
            ForkResult::Forked { child, .. } => {
                let child_id = child.id;
                let label = session_label(&parent_session);

                self.activity_log.push_simple(
                    label,
                    format!(
                        "Context overflow at {:.0}% — forking to new session",
                        overflow.context_pct * 100.0
                    ),
                    LogLevel::Warn,
                );

                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    managed.session.child_session_ids.push(child_id);
                }
                self.state.record_fork(session_id, child_id);
                self.pool.enqueue(*child);

                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::ContextOverflow,
                    ctx: HookContext::new()
                        .with_session(&session_id.to_string(), parent_session.issue_number)
                        .with_var("MAESTRO_FORK_CHILD_ID", &child_id.to_string())
                        .with_var(
                            "MAESTRO_FORK_DEPTH",
                            &(parent_session.fork_depth + 1).to_string(),
                        ),
                });

                // Mark overflow triggered to prevent re-forking
                self.context_monitor.mark_overflow_triggered(session_id);

                // Mark parent session as completed (forked) to stop resource waste
                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    managed.session.status = SessionStatus::Completed;
                    managed.session.finished_at = Some(Utc::now());
                    managed.session.current_activity = "Forked".into();
                    managed.session.log_activity(format!(
                        "Session forked to child {}",
                        &child_id.to_string()[..8]
                    ));
                }
            }
            ForkResult::Denied { reason } => {
                let label = self
                    .pool
                    .get_active_mut(session_id)
                    .map(|m| session_label(&m.session))
                    .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
                self.activity_log.push_simple(
                    label,
                    format!("Context overflow but fork denied: {}", reason),
                    LogLevel::Error,
                );
            }
        }
    }

    /// Check budget limits for a session and globally. Kill sessions if over budget.
    fn check_budget(&mut self, session_id: uuid::Uuid) {
        let Some(ref mut enforcer) = self.budget_enforcer else {
            return;
        };

        // Per-session check
        let session_cost = self
            .pool
            .get_active_mut(session_id)
            .map(|m| m.session.cost_usd)
            .unwrap_or(0.0);

        match enforcer.check_session(session_cost) {
            BudgetAction::Kill => {
                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    managed.session.status = SessionStatus::Errored;
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label,
                        format!(
                            "BUDGET EXCEEDED: ${:.2}/${:.2} per-session limit",
                            session_cost,
                            enforcer.per_session_limit()
                        ),
                        LogLevel::Error,
                    );
                }
            }
            BudgetAction::Alert(pct) => {
                if enforcer.record_alert(session_id)
                    && let Some(managed) = self.pool.get_active_mut(session_id)
                {
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label,
                        format!("Budget warning: {}% of per-session limit used", pct),
                        LogLevel::Warn,
                    );
                }
            }
            BudgetAction::Ok => {}
        }

        // Global check
        match enforcer.check_global(self.total_cost) {
            BudgetAction::Kill => {
                self.activity_log.push_simple(
                    "MAESTRO".into(),
                    format!(
                        "GLOBAL BUDGET EXCEEDED: ${:.2}/${:.2} — stopping all sessions",
                        self.total_cost,
                        enforcer.total_limit()
                    ),
                    LogLevel::Error,
                );
                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::BudgetThreshold,
                    ctx: HookContext::new()
                        .with_cost(self.total_cost)
                        .with_var("MAESTRO_BUDGET_EXCEEDED", "true"),
                });
                self.running = false;
            }
            BudgetAction::Alert(pct) => {
                if !enforcer.global_alert_sent() {
                    enforcer.mark_global_alert_sent();
                    self.activity_log.push_simple(
                        "MAESTRO".into(),
                        format!("Global budget warning: {}% used", pct),
                        LogLevel::Warn,
                    );
                }
            }
            BudgetAction::Ok => {}
        }
    }

    /// Process a data event from a background fetch task.
    pub fn handle_data_event(&mut self, evt: TuiDataEvent) {
        match evt {
            TuiDataEvent::Issues(Ok(issues)) => {
                if let Some(ref mut screen) = self.issue_browser_screen {
                    screen.set_issues(issues);
                }
            }
            TuiDataEvent::Issues(Err(e)) => {
                self.activity_log.push_simple(
                    "Issues".into(),
                    format!("Failed to fetch issues: {}", e),
                    LogLevel::Error,
                );
                if let Some(ref mut screen) = self.issue_browser_screen {
                    screen.loading = false;
                }
            }
            TuiDataEvent::Milestones(Ok(entries)) => {
                if let Some(ref mut screen) = self.milestone_screen {
                    screen.milestones = entries.into_iter().map(MilestoneEntry::from).collect();
                    screen.loading = false;
                }
            }
            TuiDataEvent::Milestones(Err(e)) => {
                self.activity_log.push_simple(
                    "Milestones".into(),
                    format!("Failed to fetch milestones: {}", e),
                    LogLevel::Error,
                );
                if let Some(ref mut screen) = self.milestone_screen {
                    screen.loading = false;
                }
            }
            TuiDataEvent::Issue(Ok(gh_issue), custom_prompt) => {
                let model = self
                    .config
                    .as_ref()
                    .map(|c| c.sessions.default_model.clone())
                    .unwrap_or_else(|| "opus".to_string());
                let default_mode = self
                    .config
                    .as_ref()
                    .map(|c| c.sessions.default_mode.clone())
                    .unwrap_or_else(|| "orchestrator".to_string());
                let issue_mode =
                    crate::modes::mode_from_labels(&gh_issue.labels).unwrap_or(default_mode);
                let issue_number = gh_issue.number;
                let base_prompt = self
                    .config
                    .as_ref()
                    .map(|c| crate::prompts::PromptBuilder::build_issue_prompt(&gh_issue, c))
                    .unwrap_or_else(|| gh_issue.unattended_prompt());
                let prompt = match custom_prompt {
                    Some(ref cp) if !cp.trim().is_empty() => {
                        format!(
                            "{}\n\n## Additional Instructions\n\n{}",
                            base_prompt,
                            cp.trim()
                        )
                    }
                    _ => base_prompt,
                };
                let mut session = Session::new(prompt, model, issue_mode, Some(issue_number));
                session.issue_title = Some(gh_issue.title.clone());
                self.state.issue_cache.insert(issue_number, gh_issue);
                self.pending_session_launches.push(session);
            }
            TuiDataEvent::Issue(Err(e), _) => {
                self.activity_log.push_simple(
                    "Session".into(),
                    format!("Failed to fetch issue: {}", e),
                    LogLevel::Error,
                );
            }
            TuiDataEvent::SuggestionData(payload) => {
                let active = self.pool.active_count();
                let suggestions = crate::tui::screens::home::Suggestion::build_suggestions(
                    payload.ready_issue_count,
                    payload.failed_issue_count,
                    &payload.milestones,
                    active,
                );
                if let Some(ref mut screen) = self.home_screen {
                    screen.set_suggestions(suggestions);
                }
            }
            TuiDataEvent::VersionCheckResult(Some(info)) => {
                self.activity_log.push_simple(
                    "UPDATE".into(),
                    format!("New version {} available", info.tag),
                    LogLevel::Info,
                );
                self.upgrade_state = crate::updater::UpgradeState::Available(info);
            }
            TuiDataEvent::VersionCheckResult(None) => {}
            TuiDataEvent::UpgradeResult(Ok(backup_path)) => {
                if let crate::updater::UpgradeState::Downloading { version } = &self.upgrade_state {
                    self.upgrade_state = crate::updater::UpgradeState::ReadyToRestart {
                        version: version.clone(),
                        backup_path,
                    };
                }
            }
            TuiDataEvent::UpgradeResult(Err(msg)) => {
                self.upgrade_state = crate::updater::UpgradeState::Failed(msg);
            }
        }
    }

    /// Check for completed sessions and promote queued ones.
    pub async fn check_completions(&mut self) -> anyhow::Result<()> {
        // Fire pending plugin hooks
        let pending_hooks = std::mem::take(&mut self.pending_hooks);
        for ph in pending_hooks {
            self.fire_plugin_hook(ph.hook, ph.ctx).await;
        }

        // Process pending issue completions (gates, git push, label updates, PR creation)
        let pending = std::mem::take(&mut self.pending_issue_completions);

        // Build gates once from config (independent of individual completions)
        let gates: Vec<CompletionGate> = if let Some(ref cfg) = self.config
            && cfg.sessions.completion_gates.enabled
            && !cfg.sessions.completion_gates.commands.is_empty()
        {
            cfg.sessions
                .completion_gates
                .commands
                .iter()
                .map(CompletionGate::from_config_entry)
                .collect()
        } else if let Some(ref cfg) = self.config
            && cfg.gates.enabled
        {
            vec![CompletionGate::TestsPass {
                command: cfg.gates.test_command.clone(),
            }]
        } else {
            vec![]
        };

        for mut completion in pending {
            let issue_label = format!("#{}", completion.issue_number);

            // Run completion gates before accepting the result
            if completion.success
                && !gates.is_empty()
                && let Some(wt_path) = &completion.worktree_path
            {
                if let Some(managed) = self.pool.find_by_issue_mut(completion.issue_number) {
                    managed.session.status = SessionStatus::GatesRunning;
                }

                let gate_runner = GateRunner;
                let results = gate_runner.run_gates(&gates, wt_path);

                let paired: Vec<(GateResult, bool)> = results
                    .into_iter()
                    .zip(gates.iter().map(|g| g.is_required()))
                    .collect();

                let all_required_passed = runner::all_required_gates_passed(&paired);

                // Log individual gate results (failed-level differs by outcome)
                let fail_level = if all_required_passed {
                    LogLevel::Warn
                } else {
                    LogLevel::Error
                };
                for (result, _) in &paired {
                    let level = if result.passed {
                        LogLevel::Info
                    } else {
                        fail_level
                    };
                    self.activity_log.push_simple(
                        issue_label.clone(),
                        format!("Gate [{}]: {}", result.gate, result.message),
                        level,
                    );
                }

                if !all_required_passed {
                    let failures: Vec<String> = paired
                        .iter()
                        .filter(|(r, required)| !r.passed && *required)
                        .map(|(r, _)| r.message.clone())
                        .collect();

                    let failed_gate_results: Vec<crate::session::types::GateResultEntry> = paired
                        .iter()
                        .filter(|(r, _)| !r.passed)
                        .map(|(r, _)| crate::session::types::GateResultEntry {
                            gate: r.gate.clone(),
                            passed: r.passed,
                            message: r.message.clone(),
                        })
                        .collect();

                    if let Some(managed) = self.pool.find_by_issue_mut(completion.issue_number) {
                        managed.session.gate_results = failed_gate_results;
                        managed.session.status = SessionStatus::NeedsReview;
                        managed
                            .session
                            .log_activity(format!("Gates failed: {}", failures.join("; ")));
                    }

                    completion.success = false;
                    let ctx = HookContext::new()
                        .with_session("", Some(completion.issue_number))
                        .with_var("MAESTRO_GATE_FAILURES", &failures.join("; "));
                    self.fire_plugin_hook(HookPoint::TestsFailed, ctx).await;
                } else {
                    self.activity_log.push_simple(
                        issue_label.clone(),
                        "All required gates passed".into(),
                        LogLevel::Info,
                    );
                    let ctx = HookContext::new().with_session("", Some(completion.issue_number));
                    self.fire_plugin_hook(HookPoint::TestsPassed, ctx).await;
                }
            }

            // If successful and we have a worktree, commit and push changes
            if completion.success
                && let (Some(branch), Some(wt_path)) =
                    (&completion.worktree_branch, &completion.worktree_path)
            {
                let git_ops = crate::git::CliGitOps;
                let commit_msg = format!(
                    "feat: implement changes for issue #{}",
                    completion.issue_number
                );
                match git_ops.commit_and_push(wt_path, branch, &commit_msg) {
                    Ok(()) => {
                        self.activity_log.push_simple(
                            format!("#{}", completion.issue_number),
                            format!("Pushed to branch {}", branch),
                            LogLevel::Info,
                        );
                    }
                    Err(e) => {
                        self.activity_log.push_simple(
                            format!("#{}", completion.issue_number),
                            format!("Git push failed: {}", e),
                            LogLevel::Error,
                        );
                    }
                }
            }

            self.on_issue_session_completed(
                completion.issue_number,
                completion.success,
                completion.cost_usd,
                completion.files_touched,
                completion.worktree_branch,
                completion.is_ci_fix,
            )
            .await;
        }

        // Stall detection: check for sessions that haven't produced events
        let stall_timeout = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.sessions.stall_timeout_secs))
            .unwrap_or(Duration::from_secs(300));

        let stalled_ids = self.health_monitor.check_stalls(stall_timeout);
        for id in &stalled_ids {
            if let Some(managed) = self.pool.get_active_mut(*id)
                && managed.session.status == SessionStatus::Running
            {
                managed.session.status = SessionStatus::Stalled;
                let label = session_label(&managed.session);
                self.activity_log.push_simple(
                    label,
                    format!(
                        "Session stalled (no activity for {}s)",
                        stall_timeout.as_secs()
                    ),
                    LogLevel::Error,
                );
            }
        }

        // Retry eligible sessions (stalled or errored) before finalizing
        let retry_policy = self
            .config
            .as_ref()
            .map(|c| RetryPolicy::new(c.sessions.max_retries, c.sessions.retry_cooldown_secs));

        let retryable_ids: Vec<uuid::Uuid> = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| matches!(s.status, SessionStatus::Stalled | SessionStatus::Errored))
            .map(|s| s.id)
            .collect();

        let mut retry_sessions = Vec::new();
        for id in &retryable_ids {
            if let Some(policy) = &retry_policy
                && let Some(managed) = self.pool.get_active_mut(*id)
                && policy.should_retry(&managed.session)
            {
                let label = session_label(&managed.session);
                // Gather progress and last error for rich retry context
                let progress = self.progress_tracker.get(id).cloned();
                let last_error = managed
                    .session
                    .activity_log
                    .iter()
                    .rev()
                    .find(|e| e.message.starts_with("ERROR:") || e.message.contains("failed"))
                    .map(|e| e.message.clone());
                let retry = policy.prepare_retry(
                    &managed.session,
                    progress.as_ref(),
                    last_error.as_deref(),
                );
                managed.session.status = SessionStatus::Retrying;
                self.activity_log.push_simple(
                    label,
                    format!(
                        "Retrying (attempt {}/{})",
                        retry.retry_count, policy.max_retries
                    ),
                    LogLevel::Warn,
                );
                retry_sessions.push(retry);
            }
        }

        // Enqueue retry sessions
        for session in retry_sessions {
            self.add_session(session).await?;
        }

        // Find terminal sessions in the active list (including Retrying which is now done)
        let completed_ids: Vec<uuid::Uuid> = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| s.status.is_terminal() || s.status == SessionStatus::Retrying)
            .map(|s| s.id)
            .collect();

        // Only process sessions that are actually in the active list
        for id in &completed_ids {
            if self.pool.get_active_mut(*id).is_some() {
                self.pool.on_session_completed(*id);
                self.health_monitor.remove(*id);
                self.progress_tracker.remove(id);
            }
        }

        // Medium tier: work assigner tick (every ~10s)
        let work_tick_interval = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.monitoring.work_tick_interval_secs))
            .unwrap_or(Duration::from_secs(10));

        if self.last_work_tick.elapsed() >= work_tick_interval {
            self.last_work_tick = Instant::now();
            // Tick the work assigner to fill available slots from GitHub issues
            self.tick_work_assigner().await?;
        }

        // Slow tier: CI status polling (every ~30s)
        self.poll_ci_status();

        // Try to promote queued sessions
        let promoted_ids = self.pool.try_promote();
        if !promoted_ids.is_empty() {
            let tx = self.pool.event_tx();
            for id in promoted_ids {
                if let Some(managed) = self.pool.get_active_mut(id) {
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label.clone(),
                        "Spawning session...".into(),
                        LogLevel::Info,
                    );
                    if let Err(e) = managed.spawn(tx.clone()).await {
                        self.activity_log.push_simple(
                            label,
                            format!("Spawn failed: {}", e),
                            LogLevel::Error,
                        );
                    } else {
                        self.activity_log.push_simple(
                            label,
                            "Session started".into(),
                            LogLevel::Info,
                        );
                    }
                }
            }
        }

        self.sync_state();
        Ok(())
    }

    /// Pause all running sessions.
    #[cfg(unix)]
    pub fn pause_all(&self) {
        self.pool.pause_all();
    }

    /// Resume all paused sessions.
    #[cfg(unix)]
    pub fn resume_all(&self) {
        self.pool.resume_all();
    }

    /// Kill all sessions.
    pub async fn kill_all(&mut self) {
        self.pool.kill_all().await;
        self.sync_state();
    }

    /// Check if all sessions are done.
    pub fn all_done(&self) -> bool {
        self.pool.all_done()
    }

    pub fn active_count(&self) -> usize {
        self.pool.active_count()
    }

    /// Build a summary of all sessions for the completion overlay.
    pub fn build_completion_summary(&self) -> CompletionSummaryData {
        use std::collections::HashMap;

        let sessions = self.pool.all_sessions();
        let mut lines = Vec::new();
        let mut total_cost = 0.0;

        let pr_map: HashMap<u64, u64> = self
            .pending_pr_checks
            .iter()
            .map(|p| (p.issue_number, p.pr_number))
            .collect();

        let repo = self
            .config
            .as_ref()
            .map(|c| c.project.repo.clone())
            .unwrap_or_default();

        for s in &sessions {
            total_cost += s.cost_usd;

            let pr_num = s
                .issue_number
                .and_then(|iss| pr_map.get(&iss).copied())
                .or_else(|| s.ci_fix_context.as_ref().map(|ctx| ctx.pr_number));

            let pr_link = match pr_num {
                Some(n) if repo.is_empty() => format!("#{}", n),
                Some(n) => format!("https://github.com/{}/pull/{}", repo, n),
                None => String::new(),
            };

            let error_summary = if s.status == SessionStatus::Errored {
                s.activity_log
                    .iter()
                    .rev()
                    .find(|e| e.message.starts_with("Error:") || e.message.starts_with("E:"))
                    .or_else(|| s.activity_log.last())
                    .map(|e| truncate_with_ellipsis(&e.message, 77))
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let gate_failures = if s.status == SessionStatus::NeedsReview {
                s.gate_results
                    .iter()
                    .filter(|r| !r.passed)
                    .map(|r| GateFailureInfo {
                        gate: r.gate.clone(),
                        message: truncate_with_ellipsis(&r.message, 100),
                    })
                    .collect()
            } else {
                Vec::new()
            };

            lines.push(CompletionSessionLine {
                label: session_label(s),
                status: s.status,
                cost_usd: s.cost_usd,
                elapsed: s.elapsed_display(),
                pr_link,
                error_summary,
                gate_failures,
                issue_number: s.issue_number,
                model: s.model.clone(),
            });
        }

        CompletionSummaryData {
            session_count: sessions.len(),
            total_cost_usd: total_cost,
            sessions: lines,
        }
    }

    /// Transition from CompletionSummary to Dashboard mode.
    pub fn transition_to_dashboard(&mut self) {
        let all = self.pool.all_sessions();

        // Save session state
        self.state.sessions = all.iter().copied().cloned().collect();
        self.state.update_total_cost();
        self.state.last_updated = Some(chrono::Utc::now());
        let _ = self.store.save(&self.state);

        // Build recent sessions for the home screen
        let recent: Vec<crate::tui::screens::home::SessionSummary> = all
            .iter()
            .rev()
            .take(10)
            .map(|s| crate::tui::screens::home::SessionSummary {
                issue_number: s.issue_number.unwrap_or(0),
                title: s
                    .issue_title
                    .clone()
                    .unwrap_or_else(|| s.last_message.clone()),
                status: s.status.label().to_string(),
                cost_usd: s.cost_usd,
            })
            .collect();

        // Initialize home screen if needed (cmd_run path has no home_screen)
        if self.home_screen.is_none() {
            let project_info = crate::tui::screens::home::ProjectInfo {
                repo: self
                    .config
                    .as_ref()
                    .map(|c| c.project.repo.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                branch: std::process::Command::new("git")
                    .args(["branch", "--show-current"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                username: None,
            };
            self.home_screen = Some(crate::tui::screens::HomeScreen::new(
                project_info,
                recent,
                Vec::new(),
            ));
        } else if let Some(ref mut screen) = self.home_screen {
            screen.recent_sessions = recent;
        }

        // Clear completion summary and switch to dashboard
        self.completion_summary = None;
        if let Some(ref mut screen) = self.home_screen {
            screen.start_loading_suggestions();
        }
        self.pending_commands.push(TuiCommand::FetchSuggestionData);
        self.tui_mode = TuiMode::Dashboard;
    }

    /// Assign ready work items from the assigner to session slots.
    pub async fn tick_work_assigner(&mut self) -> anyhow::Result<()> {
        // In continuous mode, only advance when no issue is running and not paused
        if let Some(ref cont) = self.continuous_mode
            && !cont.can_advance()
        {
            return Ok(());
        }

        // Collect ready items and mark them in-progress (scoped borrow)
        let ready_items = {
            let Some(assigner) = self.work_assigner.as_mut() else {
                return Ok(());
            };
            let Some(config) = self.config.as_ref() else {
                return Ok(());
            };

            let available_slots = self
                .pool
                .max_concurrent()
                .saturating_sub(self.pool.active_count());
            if available_slots == 0 {
                return Ok(());
            }

            let heavy_labels = &config.concurrency.heavy_task_labels;
            let heavy_limit = config.concurrency.heavy_task_limit;

            // Get all ready items, then filter by heavy task limit
            let all_ready = assigner.next_ready(available_slots);
            let mut items: Vec<(u64, String, String, String, String)> = Vec::new();
            let mut heavy_count_projected = 0usize;

            for item in all_ready {
                let is_heavy = !heavy_labels.is_empty()
                    && item.issue.labels.iter().any(|l| heavy_labels.contains(l));

                if is_heavy && heavy_count_projected >= heavy_limit {
                    // Skip — heavy task limit reached
                    continue;
                }

                let prompt = PromptBuilder::build_issue_prompt(&item.issue, config);
                let mode = item
                    .mode
                    .map(|m| m.as_config_str().to_string())
                    .unwrap_or_else(|| config.sessions.default_mode.clone());
                let model = self
                    .model_router
                    .as_ref()
                    .map(|r| r.resolve(&item.issue).to_string())
                    .unwrap_or_else(|| config.sessions.default_model.clone());

                if is_heavy {
                    heavy_count_projected += 1;
                }
                items.push((
                    item.issue.number,
                    prompt,
                    mode,
                    item.issue.title.clone(),
                    model,
                ));
            }

            // Mark in-progress within this scope
            for (issue_number, _, _, _, _) in &items {
                assigner.mark_in_progress(*issue_number);
            }

            items
        };

        let items = ready_items;

        for (issue_number, prompt, mode, title, model) in items {
            // Update GitHub labels (non-fatal on error)
            if let Some(client) = &self.github_client {
                let label_mgr = LabelManager::new(client.as_ref());
                if let Err(e) = label_mgr.mark_in_progress(issue_number).await {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Label update failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }

            let mut session = Session::new(prompt, model, mode, Some(issue_number));
            session.issue_title = Some(title);

            // Track in continuous mode
            if let Some(ref mut cont) = self.continuous_mode {
                cont.set_current_issue(issue_number);
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Advancing to next issue: #{}", issue_number),
                    LogLevel::Info,
                );
            } else {
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    "Assigned from work queue".into(),
                    LogLevel::Info,
                );
            }

            self.add_session(session).await?;
        }

        Ok(())
    }

    /// Handle completion of a session that was working on a GitHub issue.
    pub async fn on_issue_session_completed(
        &mut self,
        issue_number: u64,
        success: bool,
        cost_usd: f64,
        files_touched: Vec<String>,
        worktree_branch: Option<String>,
        is_ci_fix: bool,
    ) {
        // Update work assigner
        if let Some(ref mut assigner) = self.work_assigner {
            if success {
                let unblocked = assigner.mark_done(issue_number);
                if !unblocked.is_empty() {
                    let nums: Vec<String> = unblocked
                        .iter()
                        .map(|i| format!("#{}", i.number()))
                        .collect();
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Unblocked: {}", nums.join(", ")),
                        LogLevel::Info,
                    );
                }
            } else {
                let cascaded = assigner.mark_failed_cascade(issue_number);
                if !cascaded.is_empty() {
                    let nums: Vec<String> = cascaded.iter().map(|n| format!("#{}", n)).collect();
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Cascade failed: {}", nums.join(", ")),
                        LogLevel::Error,
                    );
                    // Emit critical notification for cascaded failures
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Critical,
                        &format!("#{} failed", issue_number),
                        &format!(
                            "Blocked {} dependent task{}: {}",
                            cascaded.len(),
                            if cascaded.len() != 1 { "s" } else { "" },
                            nums.join(", ")
                        ),
                    );
                }
            }
        }

        // Continuous mode: track completion/failure
        if let Some(ref mut cont) = self.continuous_mode {
            if success {
                cont.on_issue_completed(issue_number);
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!(
                        "Issue #{} completed ({} done so far)",
                        issue_number, cont.completed_count
                    ),
                    LogLevel::Info,
                );
            } else {
                let title = self
                    .state
                    .issue_cache
                    .get(&issue_number)
                    .map(|i| i.title.clone())
                    .unwrap_or_else(|| format!("Issue #{}", issue_number));
                let entries = self.activity_log.entries();
                let error_summary = entries
                    .iter()
                    .rev()
                    .take(10)
                    .find(|e| e.level == LogLevel::Error)
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "Session failed".into());
                cont.on_issue_failed(issue_number, title, error_summary);
                self.tui_mode = TuiMode::ContinuousPause;
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Issue #{} failed — paused for user decision", issue_number),
                    LogLevel::Warn,
                );
            }
        }

        // Update GitHub labels
        if let Some(ref client) = self.github_client {
            let label_mgr = LabelManager::new(client.as_ref());
            let result = if success {
                label_mgr.mark_done(issue_number).await
            } else {
                label_mgr.mark_failed(issue_number).await
            };
            if let Err(e) = result {
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    format!("Label update failed: {}", e),
                    LogLevel::Error,
                );
            }
        }

        // CI fix sessions skip PR creation — the PR already exists
        if is_ci_fix {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "CI fix pushed to existing PR branch".into(),
                LogLevel::Info,
            );
            return;
        }

        // Auto PR creation
        if let (Some(client), Some(config)) = (&self.github_client, &self.config)
            && success
            && config.github.auto_pr
            && let Some(ref branch) = worktree_branch
            && let Some(issue) = self.state.issue_cache.get(&issue_number)
        {
            let file_refs: Vec<&str> = files_touched.iter().map(|s| s.as_str()).collect();
            let pr_creator = PrCreator::new(client.as_ref(), config.project.base_branch.clone());
            match pr_creator
                .create_for_issue(issue, branch, &file_refs, cost_usd)
                .await
            {
                Ok(pr_num) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{} created", pr_num),
                        LogLevel::Info,
                    );
                    // Track PR for CI polling
                    if let Some(ref branch_name) = worktree_branch {
                        self.pending_pr_checks.push(PendingPrCheck {
                            pr_number: pr_num,
                            issue_number,
                            branch: branch_name.clone(),
                            created_at: Instant::now(),
                            check_count: 0,
                            fix_attempt: 0,
                            awaiting_fix_ci: false,
                        });
                    }
                    self.dispatch_review(pr_num, branch, issue_number);
                    // Fire pr_created hook
                    let ctx = HookContext::new()
                        .with_session("", Some(issue_number))
                        .with_pr(pr_num)
                        .with_branch(branch)
                        .with_cost(cost_usd);
                    self.fire_plugin_hook(HookPoint::PrCreated, ctx).await;
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR creation failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        }
    }

    /// Dispatch review for a PR if review is configured.
    fn dispatch_review(&mut self, pr_number: u64, branch: &str, issue_number: u64) {
        let Some(config) = &self.config else { return };
        let review_cfg = &config.review;
        if !review_cfg.enabled {
            return;
        }

        if !review_cfg.reviewers.is_empty() {
            let reviewers: Vec<crate::review::council::ReviewerConfig> = review_cfg
                .reviewers
                .iter()
                .map(|r| crate::review::council::ReviewerConfig {
                    name: r.name.clone(),
                    command: r.command.clone(),
                    required: r.required,
                })
                .collect();
            match crate::review::council::ReviewCouncil::convene(pr_number, branch, &reviewers) {
                Ok(council_result) => {
                    let status_label = match &council_result.status {
                        crate::review::council::ReviewStatus::Approved { .. } => "Council approved",
                        crate::review::council::ReviewStatus::Rejected { .. } => "Council rejected",
                        crate::review::council::ReviewStatus::Partial { .. } => "Council partial",
                    };
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{}: {}", pr_number, status_label),
                        LogLevel::Info,
                    );
                    let comment =
                        crate::review::council::ReviewCouncil::format_comment(&council_result);
                    let _ = crate::review::dispatch::ReviewDispatcher::post_comment(
                        pr_number, &comment,
                    );
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Council review failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        } else {
            let review_config = crate::review::ReviewConfig {
                enabled: review_cfg.enabled,
                command: review_cfg.command.clone(),
            };
            let dispatcher = crate::review::ReviewDispatcher::new(review_config);
            match dispatcher.dispatch(pr_number, branch) {
                Ok(result) => {
                    let status = if result.success {
                        "Review passed"
                    } else {
                        "Review failed"
                    };
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{}: {}", pr_number, status),
                        LogLevel::Info,
                    );
                    let comment_body = format!(
                        "**Maestro Review**\n\nStatus: {}\n\n```\n{}\n```",
                        status, result.output
                    );
                    let _ = crate::review::dispatch::ReviewDispatcher::post_comment(
                        pr_number,
                        &comment_body,
                    );
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Review dispatch failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        }
    }

    /// Poll CI status for pending PR checks. Runs on slow-tier interval.
    fn poll_ci_status(&mut self) {
        let ci_poll_interval = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.gates.ci_poll_interval_secs))
            .unwrap_or(Duration::from_secs(30));

        let ci_max_wait = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.gates.ci_max_wait_secs))
            .unwrap_or(Duration::from_secs(1800));

        if self.last_ci_poll.elapsed() < ci_poll_interval || self.pending_pr_checks.is_empty() {
            return;
        }
        self.last_ci_poll = Instant::now();

        let (auto_fix_enabled, max_retries) = self
            .config
            .as_ref()
            .map(|c| (c.gates.ci_auto_fix.enabled, c.gates.ci_auto_fix.max_retries))
            .unwrap_or((false, 3));

        let checker = CiChecker::new();
        let mut completed_indices = Vec::new();
        // Collect fix requests to process after the loop (avoids borrow conflict)
        let mut fix_requests: Vec<crate::github::ci::CiFixRequest> = Vec::new();

        for (i, check) in self.pending_pr_checks.iter_mut().enumerate() {
            check.check_count += 1;

            // Timeout check
            if check.created_at.elapsed() > ci_max_wait {
                self.activity_log.push_simple(
                    format!("PR #{}", check.pr_number),
                    format!(
                        "CI timed out after {}s",
                        check.created_at.elapsed().as_secs()
                    ),
                    LogLevel::Error,
                );
                completed_indices.push(i);
                continue;
            }

            // If awaiting a fix session's CI re-run, handle separately
            if check.awaiting_fix_ci {
                match checker.check_pr_status(check.pr_number) {
                    Ok(CiStatus::Pending) => {
                        // Fix was pushed, CI is re-running. Reset flag.
                        check.awaiting_fix_ci = false;
                    }
                    Ok(CiStatus::Passed) => {
                        check.awaiting_fix_ci = false;
                        self.activity_log.push_simple(
                            format!("PR #{}", check.pr_number),
                            format!("CI passed after {} fix attempt(s)", check.fix_attempt),
                            LogLevel::Info,
                        );
                        self.notifications.notify(
                            crate::notifications::types::InterruptLevel::Info,
                            &format!("PR #{}", check.pr_number),
                            "CI checks passed after auto-fix",
                        );
                        completed_indices.push(i);
                    }
                    Ok(CiStatus::Failed { .. }) => {
                        // Still showing old failure or fix didn't push yet. Keep waiting.
                    }
                    _ => {}
                }
                continue;
            }

            match checker.check_pr_status(check.pr_number) {
                Ok(CiStatus::Passed) => {
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        "CI passed".into(),
                        LogLevel::Info,
                    );
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Info,
                        &format!("PR #{}", check.pr_number),
                        "CI checks passed",
                    );
                    completed_indices.push(i);

                    // Auto-merge if configured
                    if let Some(ref config) = self.config
                        && config.github.auto_merge
                    {
                        let method_flag = config.github.merge_method.flag();
                        let pr_str = check.pr_number.to_string();
                        let result = std::process::Command::new("gh")
                            .args(["pr", "merge", &pr_str, method_flag, "--delete-branch"])
                            .output();
                        match result {
                            Ok(output) if output.status.success() => {
                                self.activity_log.push_simple(
                                    format!("PR #{}", check.pr_number),
                                    "Auto-merged".into(),
                                    LogLevel::Info,
                                );
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                self.activity_log.push_simple(
                                    format!("PR #{}", check.pr_number),
                                    format!("Auto-merge failed: {}", stderr.trim()),
                                    LogLevel::Error,
                                );
                            }
                            Err(e) => {
                                self.activity_log.push_simple(
                                    format!("PR #{}", check.pr_number),
                                    format!("Auto-merge error: {}", e),
                                    LogLevel::Error,
                                );
                            }
                        }
                    }
                }
                Ok(CiStatus::Failed { summary }) => {
                    use crate::github::ci::{CiFixRequest, CiPollAction, decide_ci_action};

                    let action = if auto_fix_enabled {
                        decide_ci_action(check, max_retries, &summary)
                    } else {
                        CiPollAction::Abandon
                    };

                    match action {
                        CiPollAction::SpawnFix { .. } => {
                            match checker.fetch_failure_log(check.pr_number, &check.branch) {
                                Ok(failure_log) => {
                                    self.activity_log.push_simple(
                                        format!("PR #{}", check.pr_number),
                                        format!(
                                            "CI failed (attempt {}/{}), spawning fix session",
                                            check.fix_attempt + 1,
                                            max_retries
                                        ),
                                        LogLevel::Warn,
                                    );
                                    fix_requests.push(CiFixRequest {
                                        pr_number: check.pr_number,
                                        issue_number: check.issue_number,
                                        branch: check.branch.clone(),
                                        attempt: check.fix_attempt + 1,
                                        failure_log,
                                    });
                                    check.fix_attempt += 1;
                                    check.awaiting_fix_ci = true;
                                }
                                Err(e) => {
                                    self.activity_log.push_simple(
                                        format!("PR #{}", check.pr_number),
                                        format!("CI failed, could not fetch log: {}", e),
                                        LogLevel::Error,
                                    );
                                    completed_indices.push(i);
                                }
                            }
                        }
                        CiPollAction::Abandon => {
                            self.activity_log.push_simple(
                                format!("PR #{}", check.pr_number),
                                if auto_fix_enabled {
                                    format!(
                                        "CI failed after {} fix attempts: {}",
                                        check.fix_attempt, summary
                                    )
                                } else {
                                    format!("CI failed: {}", summary)
                                },
                                LogLevel::Error,
                            );
                            self.notifications.notify(
                                crate::notifications::types::InterruptLevel::Critical,
                                &format!("PR #{} CI failed", check.pr_number),
                                &summary,
                            );
                            completed_indices.push(i);
                        }
                        CiPollAction::Wait => {} // awaiting_fix_ci handled above
                    }
                }
                Ok(CiStatus::NoneConfigured) => {
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        "No CI checks configured".into(),
                        LogLevel::Info,
                    );
                    completed_indices.push(i);
                }
                Ok(CiStatus::Pending) => {
                    // Still waiting, keep polling
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        format!("CI check error: {}", e),
                        LogLevel::Error,
                    );
                    // Don't remove — will retry next poll
                }
            }
        }

        // Spawn fix sessions after the loop to avoid borrow conflicts
        for req in fix_requests {
            self.spawn_ci_fix_session(
                req.pr_number,
                req.issue_number,
                req.branch,
                req.attempt,
                &req.failure_log,
            );
        }

        // Remove completed checks in reverse order to preserve indices
        completed_indices.sort_unstable();
        for i in completed_indices.into_iter().rev() {
            self.pending_pr_checks.remove(i);
        }
    }

    /// Spawn a Claude session to fix a CI failure on an existing PR branch.
    fn spawn_ci_fix_session(
        &mut self,
        pr_number: u64,
        issue_number: u64,
        branch: String,
        attempt: u32,
        failure_log: &str,
    ) {
        use crate::github::ci::build_ci_fix_prompt;
        use crate::session::types::CiFixContext;

        let model = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_model.clone())
            .unwrap_or_else(|| "opus".to_string());
        let mode = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_mode.clone())
            .unwrap_or_else(|| "orchestrator".to_string());

        let prompt = build_ci_fix_prompt(pr_number, issue_number, &branch, attempt, failure_log);

        let mut session = Session::new(prompt, model, mode, Some(issue_number));
        session.status = SessionStatus::CiFix;
        session.issue_title = Some(format!("CI Fix #{} for PR #{}", attempt, pr_number));
        session.ci_fix_context = Some(CiFixContext {
            pr_number,
            issue_number,
            branch,
            attempt,
        });

        self.pending_session_launches.push(session);
    }

    /// Spawn a fix session for gate failures on a NeedsReview session.
    pub fn spawn_gate_fix_session(&mut self, failed_line: &CompletionSessionLine) {
        let issue_number = match failed_line.issue_number {
            Some(n) => n,
            None => return,
        };

        let gate_failure_details: String = failed_line
            .gate_failures
            .iter()
            .map(|gf| format!("- [{}]: {}", gf.gate, gf.message))
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(2000)
            .collect();

        let model = if failed_line.model.is_empty() {
            self.config
                .as_ref()
                .map(|c| c.sessions.default_model.clone())
                .unwrap_or_else(|| "opus".to_string())
        } else {
            failed_line.model.clone()
        };

        let mode = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_mode.clone())
            .unwrap_or_else(|| "orchestrator".to_string());

        let prompt = build_gate_fix_prompt(issue_number, &gate_failure_details);

        let mut session = Session::new(prompt, model, mode, Some(issue_number));
        session.issue_title = Some(format!("Gate Fix for #{}", issue_number));

        self.pending_session_launches.push(session);

        self.activity_log.push_simple(
            format!("#{}", issue_number),
            "Launched gate fix session".into(),
            LogLevel::Info,
        );
    }

    /// Fire a plugin hook asynchronously and log results.
    pub async fn fire_plugin_hook(&mut self, hook: HookPoint, ctx: HookContext) {
        let Some(ref runner) = self.plugin_runner else {
            return;
        };
        let results = runner.fire(hook, &ctx).await;
        for result in results {
            let level = if result.success {
                LogLevel::Info
            } else {
                LogLevel::Error
            };
            let msg = if result.success {
                format!(
                    "Plugin '{}' completed ({}ms)",
                    result.plugin_name, result.duration_ms
                )
            } else {
                format!(
                    "Plugin '{}' failed: {}",
                    result.plugin_name,
                    result.output.lines().next().unwrap_or("unknown error")
                )
            };
            self.activity_log.push_simple("PLUGIN".into(), msg, level);
        }
    }

    fn sync_state(&mut self) {
        self.state.sessions = self.pool.all_sessions().into_iter().cloned().collect();
        self.state.update_total_cost();
        self.total_cost = self.state.total_cost_usd;
        self.state.last_updated = Some(Utc::now());
        let _ = self.store.save(&self.state);
    }
}

fn session_label(session: &Session) -> String {
    match session.issue_number {
        Some(n) => format!("#{}", n),
        None => format!("S-{}", &session.id.to_string()[..8]),
    }
}

fn build_gate_fix_prompt(issue_number: u64, failure_details: &str) -> String {
    format!(
        "Fix the gate failures for issue #{issue_number}.\n\n\
         GATE FAILURES:\n{failure_details}\n\n\
         IMPORTANT: You are running in unattended mode. \
         Do NOT use AskUserQuestion. \
         Read the failing code, fix the issues, then commit and push. \
         Run the failing gate commands locally to reproduce, then fix and verify. \
         Keep the fix minimal — do NOT refactor unrelated code. Only fix the gate failures."
    )
}

use crate::util::truncate_with_ellipsis;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::GateResultEntry;
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    fn make_app() -> App {
        let tmp = std::env::temp_dir().join(format!(
            "maestro-tui-app-test-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = StateStore::new(tmp);
        App::new(
            store,
            3,
            Box::new(MockWorktreeManager::new()),
            "bypassPermissions".into(),
            vec![],
        )
    }

    #[test]
    fn tui_mode_completion_summary_variant_exists() {
        let mode = TuiMode::CompletionSummary;
        assert!(matches!(mode, TuiMode::CompletionSummary));
    }

    #[test]
    fn completion_session_line_fields_are_accessible() {
        let line = CompletionSessionLine {
            label: "#42".to_string(),
            status: crate::session::types::SessionStatus::Completed,
            cost_usd: 1.23,
            elapsed: "1m 05s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![],
            issue_number: Some(42),
            model: "opus".to_string(),
        };
        assert_eq!(line.label, "#42");
        assert_eq!(line.status, crate::session::types::SessionStatus::Completed);
        assert!((line.cost_usd - 1.23).abs() < f64::EPSILON);
        assert_eq!(line.elapsed, "1m 05s");
    }

    #[test]
    fn completion_summary_data_fields_are_accessible() {
        let data = CompletionSummaryData {
            sessions: vec![],
            total_cost_usd: 0.0,
            session_count: 0,
        };
        assert!(data.sessions.is_empty());
        assert_eq!(data.session_count, 0);
        assert!(data.total_cost_usd.abs() < f64::EPSILON);
    }

    #[test]
    fn app_once_mode_defaults_to_false() {
        let app = make_app();
        assert!(!app.once_mode, "once_mode must default to false");
    }

    #[test]
    fn app_completion_summary_defaults_to_none() {
        let app = make_app();
        assert!(
            app.completion_summary.is_none(),
            "completion_summary must default to None"
        );
    }

    #[test]
    fn build_completion_summary_returns_empty_when_no_sessions() {
        let app = make_app();
        let summary = app.build_completion_summary();
        assert_eq!(summary.session_count, 0);
        assert!(summary.sessions.is_empty());
        assert!(summary.total_cost_usd.abs() < f64::EPSILON);
    }

    #[test]
    fn build_completion_summary_label_uses_issue_number_when_present() {
        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "do something".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(99),
        );
        app.pool.enqueue(session);
        let summary = app.build_completion_summary();
        assert_eq!(summary.session_count, 1);
        assert!(
            summary.sessions[0].label.contains("#99"),
            "label must include issue number"
        );
    }

    #[test]
    fn build_completion_summary_label_uses_short_id_when_no_issue() {
        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "do something".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
        );
        let short_id = session.id.to_string()[..8].to_string();
        app.pool.enqueue(session);
        let summary = app.build_completion_summary();
        assert_eq!(summary.session_count, 1);
        assert!(
            summary.sessions[0].label.contains(&short_id),
            "label must include short UUID when no issue"
        );
    }

    #[test]
    fn build_completion_summary_aggregates_cost() {
        let mut app = make_app();
        let mut s1 = crate::session::types::Session::new(
            "task 1".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
        );
        s1.cost_usd = 1.50;
        let mut s2 = crate::session::types::Session::new(
            "task 2".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(2),
        );
        s2.cost_usd = 2.75;
        app.pool.enqueue(s1);
        app.pool.enqueue(s2);
        let summary = app.build_completion_summary();
        assert!((summary.total_cost_usd - 4.25).abs() < 0.001);
        assert_eq!(summary.session_count, 2);
    }

    #[test]
    fn transition_to_dashboard_sets_tui_mode_to_dashboard() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.transition_to_dashboard();
        assert!(matches!(app.tui_mode, TuiMode::Dashboard));
    }

    #[test]
    fn transition_to_dashboard_clears_completion_summary() {
        let mut app = make_app();
        app.completion_summary = Some(CompletionSummaryData {
            sessions: vec![],
            total_cost_usd: 0.0,
            session_count: 0,
        });
        app.transition_to_dashboard();
        assert!(app.completion_summary.is_none());
    }

    #[test]
    fn transition_to_dashboard_preserves_orthogonal_state() {
        let mut app = make_app();
        app.total_cost = 9.99;
        app.running = true;
        app.transition_to_dashboard();
        assert!(app.running);
        assert!((app.total_cost - 9.99).abs() < f64::EPSILON);
    }

    #[test]
    fn transition_to_dashboard_creates_home_screen_when_missing() {
        let mut app = make_app();
        assert!(app.home_screen.is_none());
        app.transition_to_dashboard();
        assert!(app.home_screen.is_some());
    }

    #[test]
    fn transition_to_dashboard_queues_suggestion_refresh() {
        let mut app = make_app();
        app.transition_to_dashboard();
        assert!(
            app.pending_commands
                .iter()
                .any(|c| matches!(c, TuiCommand::FetchSuggestionData)),
            "must queue FetchSuggestionData"
        );
    }

    // --- Issue #86: suggestion refresh after session completion ---

    #[test]
    fn transition_to_dashboard_sets_loading_suggestions_flag() {
        let mut app = make_app();
        app.transition_to_dashboard();
        assert!(
            app.home_screen
                .as_ref()
                .map(|s| s.loading_suggestions)
                .unwrap_or(false),
            "transition_to_dashboard must set loading_suggestions = true"
        );
    }

    #[test]
    fn suggestion_data_event_clears_loading_flag_on_home_screen() {
        let mut app = make_app();
        app.transition_to_dashboard();
        if let Some(ref mut screen) = app.home_screen {
            screen.loading_suggestions = true;
        }
        app.handle_data_event(TuiDataEvent::SuggestionData(SuggestionDataPayload {
            ready_issue_count: 0,
            failed_issue_count: 0,
            milestones: vec![],
        }));
        assert!(
            !app.home_screen.as_ref().unwrap().loading_suggestions,
            "SuggestionData event must clear loading_suggestions"
        );
    }

    // --- Issue #84: post-session activity log with cost summary ---

    #[test]
    fn completion_session_line_pr_link_defaults_to_empty() {
        let line = CompletionSessionLine {
            label: "#1".to_string(),
            status: crate::session::types::SessionStatus::Completed,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![],
            issue_number: Some(1),
            model: "opus".to_string(),
        };
        assert!(line.pr_link.is_empty());
    }

    #[test]
    fn completion_session_line_holds_pr_link_value() {
        let line = CompletionSessionLine {
            label: "#42".to_string(),
            status: crate::session::types::SessionStatus::Completed,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: "https://github.com/org/repo/pull/42".into(),
            error_summary: String::new(),
            gate_failures: vec![],
            issue_number: Some(42),
            model: "opus".to_string(),
        };
        assert_eq!(line.pr_link, "https://github.com/org/repo/pull/42");
    }

    #[test]
    fn completion_session_line_holds_error_summary_value() {
        let line = CompletionSessionLine {
            label: "#7".to_string(),
            status: crate::session::types::SessionStatus::Errored,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: "Error: process exited with code 1".into(),
            gate_failures: vec![],
            issue_number: Some(7),
            model: "opus".to_string(),
        };
        assert_eq!(line.error_summary, "Error: process exited with code 1");
    }

    #[test]
    fn build_completion_summary_sets_pr_link_when_pending_check_matches() {
        use crate::github::ci::PendingPrCheck;

        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(10),
        );
        session.status = crate::session::types::SessionStatus::Completed;
        app.pool.enqueue(session);
        app.pending_pr_checks.push(PendingPrCheck {
            pr_number: 42,
            issue_number: 10,
            branch: "feat/issue-10".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 0,
            awaiting_fix_ci: false,
        });

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].pr_link.contains("42"),
            "pr_link must reference PR number"
        );
    }

    #[test]
    fn build_completion_summary_pr_link_empty_when_no_matching_check() {
        use crate::github::ci::PendingPrCheck;

        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(99),
        );
        app.pool.enqueue(session);
        app.pending_pr_checks.push(PendingPrCheck {
            pr_number: 5,
            issue_number: 5,
            branch: "feat/issue-5".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 0,
            awaiting_fix_ci: false,
        });

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].pr_link.is_empty(),
            "pr_link must be empty when no PendingPrCheck matches"
        );
    }

    #[test]
    fn build_completion_summary_pr_link_empty_when_no_issue_number() {
        use crate::github::ci::PendingPrCheck;

        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
        );
        app.pool.enqueue(session);
        app.pending_pr_checks.push(PendingPrCheck {
            pr_number: 1,
            issue_number: 1,
            branch: "feat/issue-1".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 0,
            awaiting_fix_ci: false,
        });

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].pr_link.is_empty(),
            "pr_link must be empty for sessions without issue_number"
        );
    }

    #[test]
    fn build_completion_summary_error_summary_for_errored_session() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(5),
        );
        session.status = crate::session::types::SessionStatus::Errored;
        session.log_activity("Process started".into());
        session.log_activity("Error: process exited with code 1".into());
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(
            !summary.sessions[0].error_summary.is_empty(),
            "error_summary must be set for Errored sessions with activity"
        );
    }

    #[test]
    fn build_completion_summary_error_summary_empty_for_completed() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(6),
        );
        session.status = crate::session::types::SessionStatus::Completed;
        session.log_activity("Some activity".into());
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].error_summary.is_empty(),
            "error_summary must be empty for Completed sessions"
        );
    }

    #[test]
    fn build_completion_summary_error_summary_empty_when_no_activity() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(7),
        );
        session.status = crate::session::types::SessionStatus::Errored;
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].error_summary.is_empty(),
            "error_summary must be empty when activity_log is empty"
        );
    }

    #[test]
    fn build_completion_summary_error_summary_truncates_long_messages() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(8),
        );
        session.status = crate::session::types::SessionStatus::Errored;
        session.log_activity(format!("Error: {}", "x".repeat(200)));
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        let err = &summary.sessions[0].error_summary;
        assert!(
            err.len() <= 83,
            "error_summary must be truncated, got {} chars",
            err.len()
        );
        assert!(err.ends_with("..."), "truncated summary must end with ...");
    }

    #[test]
    fn build_completion_summary_pr_link_from_ci_fix_context() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "fix ci".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(15),
        );
        session.status = crate::session::types::SessionStatus::Completed;
        session.ci_fix_context = Some(crate::session::types::CiFixContext {
            pr_number: 77,
            issue_number: 15,
            branch: "feat/fix-ci".into(),
            attempt: 1,
        });
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].pr_link.contains("77"),
            "pr_link must reference ci_fix_context PR number"
        );
    }

    // --- Issue #104: [f] Fix action in completion overlay for failed gates ---

    #[test]
    fn gate_failure_info_fields_are_accessible() {
        let info = GateFailureInfo {
            gate: "tests".to_string(),
            message: "3 tests failed".to_string(),
        };
        assert_eq!(info.gate, "tests");
        assert_eq!(info.message, "3 tests failed");
    }

    #[test]
    fn gate_failure_info_can_be_cloned() {
        let info = GateFailureInfo {
            gate: "clippy".to_string(),
            message: "2 warnings".to_string(),
        };
        let cloned = info.clone();
        assert_eq!(cloned.gate, info.gate);
        assert_eq!(cloned.message, info.message);
    }

    #[test]
    fn completion_session_line_gate_failures_defaults_to_empty() {
        let line = CompletionSessionLine {
            label: "#42".to_string(),
            status: crate::session::types::SessionStatus::NeedsReview,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![],
            issue_number: Some(42),
            model: "opus".to_string(),
        };
        assert!(line.gate_failures.is_empty());
    }

    #[test]
    fn completion_session_line_holds_gate_failures() {
        let line = CompletionSessionLine {
            label: "#7".to_string(),
            status: crate::session::types::SessionStatus::NeedsReview,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![GateFailureInfo {
                gate: "tests".to_string(),
                message: "cargo test failed".to_string(),
            }],
            issue_number: Some(7),
            model: "opus".to_string(),
        };
        assert_eq!(line.gate_failures.len(), 1);
        assert_eq!(line.gate_failures[0].gate, "tests");
    }

    #[test]
    fn has_needs_review_returns_false_when_no_sessions() {
        let data = CompletionSummaryData {
            sessions: vec![],
            total_cost_usd: 0.0,
            session_count: 0,
        };
        assert!(!data.has_needs_review());
    }

    #[test]
    fn has_needs_review_returns_false_when_all_completed() {
        let data = CompletionSummaryData {
            sessions: vec![CompletionSessionLine {
                label: "#1".to_string(),
                status: crate::session::types::SessionStatus::Completed,
                cost_usd: 0.0,
                elapsed: "0s".to_string(),
                pr_link: String::new(),
                error_summary: String::new(),
                gate_failures: vec![],
                issue_number: Some(1),
                model: "opus".to_string(),
            }],
            total_cost_usd: 0.0,
            session_count: 1,
        };
        assert!(!data.has_needs_review());
    }

    #[test]
    fn has_needs_review_returns_true_when_one_session_needs_review() {
        let data = CompletionSummaryData {
            sessions: vec![CompletionSessionLine {
                label: "#2".to_string(),
                status: crate::session::types::SessionStatus::NeedsReview,
                cost_usd: 0.0,
                elapsed: "0s".to_string(),
                pr_link: String::new(),
                error_summary: String::new(),
                gate_failures: vec![GateFailureInfo {
                    gate: "tests".to_string(),
                    message: "failed".to_string(),
                }],
                issue_number: Some(2),
                model: "opus".to_string(),
            }],
            total_cost_usd: 0.0,
            session_count: 1,
        };
        assert!(data.has_needs_review());
    }

    #[test]
    fn has_needs_review_returns_true_when_mixed_statuses() {
        let data = CompletionSummaryData {
            sessions: vec![
                CompletionSessionLine {
                    label: "#1".to_string(),
                    status: crate::session::types::SessionStatus::Completed,
                    cost_usd: 0.0,
                    elapsed: "0s".to_string(),
                    pr_link: String::new(),
                    error_summary: String::new(),
                    gate_failures: vec![],
                    issue_number: Some(1),
                    model: "opus".to_string(),
                },
                CompletionSessionLine {
                    label: "#2".to_string(),
                    status: crate::session::types::SessionStatus::NeedsReview,
                    cost_usd: 0.0,
                    elapsed: "0s".to_string(),
                    pr_link: String::new(),
                    error_summary: String::new(),
                    gate_failures: vec![GateFailureInfo {
                        gate: "clippy".to_string(),
                        message: "lint error".to_string(),
                    }],
                    issue_number: Some(2),
                    model: "opus".to_string(),
                },
            ],
            total_cost_usd: 0.0,
            session_count: 2,
        };
        assert!(data.has_needs_review());
    }

    #[test]
    fn build_completion_summary_gate_failures_empty_for_completed_session() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(10),
        );
        session.status = crate::session::types::SessionStatus::Completed;
        session.gate_results = vec![GateResultEntry::pass("tests", "all passed")];
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(
            summary.sessions[0].gate_failures.is_empty(),
            "completed session with passing gates must have empty gate_failures"
        );
    }

    #[test]
    fn build_completion_summary_gate_failures_populated_for_needs_review() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(20),
        );
        session.status = crate::session::types::SessionStatus::NeedsReview;
        session.gate_results = vec![GateResultEntry::fail("tests", "3 tests failed")];
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert_eq!(summary.sessions[0].gate_failures.len(), 1);
        assert_eq!(summary.sessions[0].gate_failures[0].gate, "tests");
        assert!(
            summary.sessions[0].gate_failures[0]
                .message
                .contains("3 tests failed")
        );
    }

    #[test]
    fn build_completion_summary_gate_failures_multiple_failed_gates() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(30),
        );
        session.status = crate::session::types::SessionStatus::NeedsReview;
        session.gate_results = vec![
            GateResultEntry::fail("tests", "cargo test failed"),
            GateResultEntry::fail("clippy", "2 warnings"),
        ];
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert_eq!(summary.sessions[0].gate_failures.len(), 2);
    }

    #[test]
    fn build_completion_summary_gate_failures_skips_passing_gates() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(40),
        );
        session.status = crate::session::types::SessionStatus::NeedsReview;
        session.gate_results = vec![
            GateResultEntry::pass("fmt", "formatted"),
            GateResultEntry::fail("tests", "failed"),
        ];
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert_eq!(summary.sessions[0].gate_failures.len(), 1);
        assert_eq!(summary.sessions[0].gate_failures[0].gate, "tests");
    }

    #[test]
    fn build_completion_summary_gate_failures_empty_when_needs_review_has_no_gate_results() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(50),
        );
        session.status = crate::session::types::SessionStatus::NeedsReview;
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(summary.sessions[0].gate_failures.is_empty());
    }

    #[test]
    fn build_completion_summary_populates_issue_number() {
        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(77),
        );
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert_eq!(summary.sessions[0].issue_number, Some(77));
    }

    #[test]
    fn build_completion_summary_issue_number_is_none_when_session_has_none() {
        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
        );
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert!(summary.sessions[0].issue_number.is_none());
    }

    #[test]
    fn build_completion_summary_populates_model() {
        let mut app = make_app();
        let session = crate::session::types::Session::new(
            "task".into(),
            "claude-opus-4".into(),
            "orchestrator".into(),
            Some(88),
        );
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        assert_eq!(summary.sessions[0].model, "claude-opus-4");
    }

    #[test]
    fn build_completion_summary_gate_failure_message_truncated() {
        let mut app = make_app();
        let mut session = crate::session::types::Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(60),
        );
        session.status = crate::session::types::SessionStatus::NeedsReview;
        session.gate_results = vec![GateResultEntry::fail("tests", &"x".repeat(300))];
        app.pool.enqueue(session);

        let summary = app.build_completion_summary();
        let msg = &summary.sessions[0].gate_failures[0].message;
        assert!(
            msg.len() <= 104,
            "gate failure message must be truncated, got {} chars",
            msg.len()
        );
    }

    #[test]
    fn build_gate_fix_prompt_includes_issue_number() {
        let prompt = build_gate_fix_prompt(42, "tests failed");
        assert!(prompt.contains("42"));
    }

    #[test]
    fn build_gate_fix_prompt_includes_failure_details() {
        let details = "- [tests]: cargo test -- 3 failures";
        let prompt = build_gate_fix_prompt(10, details);
        assert!(prompt.contains(details));
    }

    #[test]
    fn build_gate_fix_prompt_is_non_empty() {
        let prompt = build_gate_fix_prompt(99, "gate failed");
        assert!(!prompt.is_empty());
    }

    #[test]
    fn spawn_gate_fix_session_queues_pending_launch() {
        let mut app = make_app();

        let line = CompletionSessionLine {
            label: "#55".to_string(),
            status: crate::session::types::SessionStatus::NeedsReview,
            cost_usd: 1.0,
            elapsed: "30s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![GateFailureInfo {
                gate: "tests".to_string(),
                message: "cargo test failed".to_string(),
            }],
            issue_number: Some(55),
            model: "opus".to_string(),
        };

        app.spawn_gate_fix_session(&line);
        assert!(
            !app.pending_session_launches.is_empty(),
            "spawn_gate_fix_session must queue a session launch"
        );
    }

    #[test]
    fn spawn_gate_fix_session_does_nothing_when_no_issue_number() {
        let mut app = make_app();
        let line = CompletionSessionLine {
            label: "abc123".to_string(),
            status: crate::session::types::SessionStatus::NeedsReview,
            cost_usd: 0.0,
            elapsed: "0s".to_string(),
            pr_link: String::new(),
            error_summary: String::new(),
            gate_failures: vec![GateFailureInfo {
                gate: "tests".to_string(),
                message: "failed".to_string(),
            }],
            issue_number: None,
            model: "opus".to_string(),
        };

        app.spawn_gate_fix_session(&line);
        assert!(
            app.pending_session_launches.is_empty(),
            "spawn_gate_fix_session must be a no-op when issue_number is None"
        );
    }
}
