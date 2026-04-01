use crate::budget::{BudgetAction, BudgetCheck, BudgetEnforcer};
use crate::config::Config;
use crate::gates::runner::{self, GateCheck, GateRunner};
use crate::gates::types::CompletionGate;
use crate::git::GitOps;
use crate::github::ci::{CiChecker, CiStatus, PendingPrCheck};
use crate::github::client::GitHubClient;
use crate::github::labels::LabelManager;
use crate::github::pr::PrCreator;
use crate::models::ModelRouter;
use crate::notifications::dispatcher::NotificationDispatcher;
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
    pub event_tx: mpsc::UnboundedSender<SessionEvent>,
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
    /// Context overflow monitor.
    pub context_monitor: Box<dyn ContextMonitor>,
    /// Fork policy for auto-fork decisions.
    pub fork_policy: Option<ForkPolicy>,
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
            event_tx,
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
            context_monitor: Box::new(ProductionContextMonitor::new()),
            fork_policy: None,
        }
    }

    /// Configure the app with a loaded Config, setting up fork policy and other config-dependent fields.
    pub fn configure(&mut self, config: Config) {
        self.fork_policy = Some(ForkPolicy::new(
            config.sessions.context_overflow.max_fork_depth,
        ));
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
                self.activity_log.push_simple(
                    label,
                    format!(
                        "CONFLICT: {} claimed by S-{}",
                        path,
                        &owner.to_string()[..8]
                    ),
                    LogLevel::Error,
                );
                // Queue file_conflict hook
                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::FileConflict,
                    ctx: HookContext::new()
                        .with_session(&session_id.to_string(), None)
                        .with_var("MAESTRO_CONFLICT_FILE", path)
                        .with_var("MAESTRO_CONFLICT_OWNER", &owner.to_string()),
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
                    tool, file_path, ..
                } => {
                    self.activity_log
                        .push_simple(label, format!("Using {}", tool), LogLevel::Tool);
                    // Track progress phase
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_tool_use(tool, file_path.as_deref());
                }
                StreamEvent::AssistantMessage { text } => {
                    let preview = if text.len() > 60 {
                        let end = truncate_at_char_boundary(text, 60);
                        format!("{}…", &text[..end])
                    } else {
                        text.clone()
                    };
                    if !preview.is_empty() {
                        self.activity_log.push_simple(
                            label,
                            format!("\"{}\"", preview),
                            LogLevel::Info,
                        );
                    }
                    // Track progress phase from message content
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_message(text);
                }
                StreamEvent::Completed { cost_usd } => {
                    self.activity_log.push_simple(
                        label,
                        format!("Completed (${:.2})", cost_usd),
                        LogLevel::Info,
                    );
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
                        });
                    }
                }
                StreamEvent::Error { message } => {
                    self.activity_log.push_simple(
                        label,
                        format!("ERROR: {}", message),
                        LogLevel::Error,
                    );
                    // Queue issue failure for async processing
                    if let Some(issue_num) = managed.session.issue_number {
                        self.pending_issue_completions.push(PendingIssueCompletion {
                            issue_number: issue_num,
                            success: false,
                            cost_usd: managed.session.cost_usd,
                            files_touched: managed.session.files_touched.clone(),
                            worktree_branch: managed.branch_name.clone(),
                            worktree_path: managed.worktree_path.clone(),
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

    /// Check for completed sessions and promote queued ones.
    pub async fn check_completions(&mut self) -> anyhow::Result<()> {
        // Fire pending plugin hooks
        let pending_hooks = std::mem::take(&mut self.pending_hooks);
        for ph in pending_hooks {
            self.fire_plugin_hook(ph.hook, ph.ctx).await;
        }

        // Process pending issue completions (gates, git push, label updates, PR creation)
        let pending = std::mem::take(&mut self.pending_issue_completions);
        let gates_config = self.config.as_ref().map(|c| c.gates.clone());

        for mut completion in pending {
            // Run completion gates before accepting the result
            if completion.success
                && let (Some(gates_cfg), Some(wt_path)) = (&gates_config, &completion.worktree_path)
                && gates_cfg.enabled
            {
                let gates = vec![CompletionGate::TestsPass {
                    command: gates_cfg.test_command.clone(),
                }];
                let gate_runner = GateRunner;
                let results = gate_runner.run_gates(&gates, wt_path);

                if !runner::all_gates_passed(&results) {
                    let failures: Vec<String> = results
                        .iter()
                        .filter(|r| !r.passed)
                        .map(|r| r.message.clone())
                        .collect();
                    self.activity_log.push_simple(
                        format!("#{}", completion.issue_number),
                        format!("Gate failed: {}", failures.join("; ")),
                        LogLevel::Error,
                    );
                    // Mark as failed — retry system will pick it up
                    completion.success = false;
                    // Fire tests_failed hook
                    let ctx = HookContext::new()
                        .with_session("", Some(completion.issue_number))
                        .with_var("MAESTRO_GATE_FAILURES", &failures.join("; "));
                    self.fire_plugin_hook(HookPoint::TestsFailed, ctx).await;
                } else {
                    self.activity_log.push_simple(
                        format!("#{}", completion.issue_number),
                        "All gates passed".into(),
                        LogLevel::Info,
                    );
                    // Fire tests_passed hook
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

    /// Assign ready work items from the assigner to session slots.
    pub async fn tick_work_assigner(&mut self) -> anyhow::Result<()> {
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

            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "Assigned from work queue".into(),
                LogLevel::Info,
            );

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
                auto_approve: review_cfg.auto_approve,
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

        let checker = CiChecker::new();
        let mut completed_indices = Vec::new();

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
                    self.activity_log.push_simple(
                        format!("PR #{}", check.pr_number),
                        format!("CI failed: {}", summary),
                        LogLevel::Error,
                    );
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Critical,
                        &format!("PR #{} CI failed", check.pr_number),
                        &summary,
                    );
                    completed_indices.push(i);
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

        // Remove completed checks in reverse order to preserve indices
        completed_indices.sort_unstable();
        for i in completed_indices.into_iter().rev() {
            self.pending_pr_checks.remove(i);
        }
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

use crate::util::truncate_at_char_boundary;
