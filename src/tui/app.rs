use crate::session::manager::SessionEvent;
use crate::session::pool::SessionPool;
use crate::session::types::{Session, StreamEvent};
use crate::session::worktree::WorktreeManager;
use crate::state::file_claims::{ClaimResult, FILE_CONFLICT_SENTINEL};
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use crate::tui::activity_log::{ActivityLog, LogLevel};
use crate::tui::panels::PanelView;
use chrono::Utc;
use tokio::sync::mpsc;

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
                self.activity_log
                    .push_simple(session_label.clone(), "Spawning session...".into(), LogLevel::Info);
                if let Err(e) = managed.spawn(tx.clone()).await {
                    self.activity_log.push_simple(
                        session_label,
                        format!("Spawn failed: {}", e),
                        LogLevel::Error,
                    );
                } else {
                    self.activity_log
                        .push_simple(session_label, "Session started".into(), LogLevel::Info);
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
                        self.activity_log
                            .push_simple(label, format!("\"{}\"", preview), LogLevel::Info);
                    }
                }
                StreamEvent::Completed { cost_usd } => {
                    self.activity_log.push_simple(
                        label,
                        format!("Completed (${:.2})", cost_usd),
                        LogLevel::Info,
                    );
                }
                StreamEvent::Error { message } => {
                    self.activity_log
                        .push_simple(label, format!("ERROR: {}", message), LogLevel::Error);
                }
                _ => {}
            }
        }

        self.sync_state();
    }

    /// Check for completed sessions and promote queued ones.
    pub async fn check_completions(&mut self) -> anyhow::Result<()> {
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

        // Try to promote queued sessions
        let promoted_ids = self.pool.try_promote();
        if !promoted_ids.is_empty() {
            let tx = self.pool.event_tx();
            for id in promoted_ids {
                if let Some(managed) = self.pool.get_active_mut(id) {
                    let label = session_label(&managed.session);
                    self.activity_log
                        .push_simple(label.clone(), "Spawning session...".into(), LogLevel::Info);
                    if let Err(e) = managed.spawn(tx.clone()).await {
                        self.activity_log.push_simple(
                            label,
                            format!("Spawn failed: {}", e),
                            LogLevel::Error,
                        );
                    } else {
                        self.activity_log
                            .push_simple(label, "Session started".into(), LogLevel::Info);
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

    fn sync_state(&mut self) {
        self.state.sessions = self
            .pool
            .all_sessions()
            .into_iter()
            .cloned()
            .collect();
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

/// Find the largest byte offset <= max_bytes that is a valid char boundary.
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> usize {
    if s.len() <= max_bytes {
        return s.len();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    end
}
