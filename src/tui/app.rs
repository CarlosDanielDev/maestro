use crate::config::Config;
use crate::github::client::GitHubClient;
use crate::github::labels::LabelManager;
use crate::github::pr::PrCreator;
use crate::session::manager::SessionEvent;
use crate::session::pool::SessionPool;
use crate::session::types::{Session, StreamEvent};
use crate::session::worktree::WorktreeManager;
use crate::state::file_claims::{ClaimResult, FILE_CONFLICT_SENTINEL};
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use crate::tui::activity_log::{ActivityLog, LogLevel};
use crate::tui::panels::PanelView;
use crate::work::assigner::WorkAssigner;
use chrono::Utc;
use tokio::sync::mpsc;

struct PendingIssueCompletion {
    issue_number: u64,
    success: bool,
    cost_usd: f64,
    files_touched: Vec<String>,
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
        }
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
                }
            }
        }

        self.sync_state();
        Ok(())
    }

    /// Process a stream event from a session.
    pub fn handle_session_event(&mut self, evt: SessionEvent) {
        let session_id = evt.session_id;

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
                StreamEvent::ToolUse { tool, .. } => {
                    self.activity_log
                        .push_simple(label, format!("Using {}", tool), LogLevel::Tool);
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
                }
                StreamEvent::Completed { cost_usd } => {
                    self.activity_log.push_simple(
                        label,
                        format!("Completed (${:.2})", cost_usd),
                        LogLevel::Info,
                    );
                    // Queue issue completion for async processing
                    if let Some(issue_num) = managed.session.issue_number {
                        self.pending_issue_completions.push(PendingIssueCompletion {
                            issue_number: issue_num,
                            success: true,
                            cost_usd: *cost_usd,
                            files_touched: managed.session.files_touched.clone(),
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
                        });
                    }
                }
                _ => {}
            }
        }

        self.sync_state();
    }

    /// Check for completed sessions and promote queued ones.
    pub async fn check_completions(&mut self) -> anyhow::Result<()> {
        // Process pending issue completions (label updates, PR creation)
        let pending = std::mem::take(&mut self.pending_issue_completions);
        for completion in pending {
            self.on_issue_session_completed(
                completion.issue_number,
                completion.success,
                completion.cost_usd,
                completion.files_touched,
                None, // TODO: pass worktree branch when available
            )
            .await;
        }

        // Find terminal sessions in the active list
        let completed_ids: Vec<uuid::Uuid> = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| s.status.is_terminal())
            .map(|s| s.id)
            .collect();

        // Only process sessions that are actually in the active list
        for id in &completed_ids {
            if self.pool.get_active_mut(*id).is_some() {
                self.pool.on_session_completed(*id);
            }
        }

        // Tick the work assigner to fill available slots from GitHub issues
        self.tick_work_assigner().await?;

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

            let items: Vec<(u64, String, String, String)> = assigner
                .next_ready(available_slots)
                .iter()
                .map(|item| {
                    let prompt = item.issue.unattended_prompt();
                    let mode = item
                        .mode
                        .map(|m| m.as_config_str().to_string())
                        .unwrap_or_else(|| config.sessions.default_mode.clone());
                    (item.issue.number, prompt, mode, item.issue.title.clone())
                })
                .collect();

            // Mark in-progress within this scope
            for (issue_number, _, _, _) in &items {
                assigner.mark_in_progress(*issue_number);
            }

            let model = config.sessions.default_model.clone();
            (items, model)
        };

        let (items, model) = ready_items;

        for (issue_number, prompt, mode, title) in items {
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

            let mut session = Session::new(prompt, model.clone(), mode, Some(issue_number));
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
                assigner.mark_failed(issue_number);
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
