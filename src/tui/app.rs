use crate::session::manager::{ManagedSession, SessionEvent};
use crate::session::types::{Session, SessionStatus, StreamEvent};
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use chrono::Utc;
use tokio::sync::mpsc;

/// Global activity log entry displayed in the bottom panel.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub session_label: String,
    pub message: String,
}

pub struct App {
    pub sessions: Vec<ManagedSession>,
    pub activity_log: Vec<LogEntry>,
    pub state: MaestroState,
    pub store: StateStore,
    pub running: bool,
    pub total_cost: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub event_tx: mpsc::UnboundedSender<SessionEvent>,
    pub event_rx: mpsc::UnboundedReceiver<SessionEvent>,
}

impl App {
    pub fn new(store: StateStore) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let state = store.load().unwrap_or_default();
        Self {
            sessions: Vec::new(),
            activity_log: Vec::new(),
            state,
            store,
            running: true,
            total_cost: 0.0,
            start_time: Utc::now(),
            event_tx,
            event_rx,
        }
    }

    /// Add a session and spawn it.
    pub async fn add_session(&mut self, session: Session) -> anyhow::Result<()> {
        let label = session_label(&session);
        let mut managed = ManagedSession::new(session);

        self.push_log(&label, "Spawning session…");

        managed.spawn(self.event_tx.clone()).await?;
        self.push_log(&label, "Session started");

        self.state.sessions.push(managed.session.clone());
        self.sessions.push(managed);
        self.save_state();
        Ok(())
    }

    /// Process a stream event from a session.
    pub fn handle_session_event(&mut self, evt: SessionEvent) {
        // First: update the managed session and extract what we need for logging
        let log_msg = {
            let Some(managed) = self
                .sessions
                .iter_mut()
                .find(|s| s.session.id == evt.session_id)
            else {
                return;
            };

            managed.handle_event(&evt.event);

            let label = session_label(&managed.session);

            // Build log message
            let msg = match &evt.event {
                StreamEvent::ToolUse { tool, .. } => Some(format!("Using {}", tool)),
                StreamEvent::AssistantMessage { text } => {
                    let preview = if text.len() > 60 {
                        format!("{}…", &text[..60])
                    } else {
                        text.clone()
                    };
                    if preview.is_empty() {
                        None
                    } else {
                        Some(format!("\"{}\"", preview))
                    }
                }
                StreamEvent::Completed { cost_usd } => {
                    Some(format!("Completed (${:.2})", cost_usd))
                }
                StreamEvent::Error { message } => Some(format!("ERROR: {}", message)),
                _ => None,
            };

            msg.map(|m| (label, m))
        };

        // Second: push log (no longer borrowing self.sessions)
        if let Some((label, message)) = log_msg {
            self.push_log(&label, &message);
        }

        // Third: sync state snapshot
        let session_id = evt.session_id;
        if let Some(managed) = self
            .sessions
            .iter()
            .find(|s| s.session.id == session_id)
        {
            let cloned = managed.session.clone();
            if let Some(state_session) = self
                .state
                .sessions
                .iter_mut()
                .find(|s| s.id == session_id)
            {
                *state_session = cloned;
            }
        }
        self.state.update_total_cost();
        self.total_cost = self.state.total_cost_usd;
    }

    /// Pause all running sessions.
    #[cfg(unix)]
    pub fn pause_all(&self) {
        for managed in &self.sessions {
            if managed.session.status == SessionStatus::Running {
                let _ = managed.pause();
            }
        }
    }

    /// Resume all paused sessions.
    #[cfg(unix)]
    pub fn resume_all(&self) {
        for managed in &self.sessions {
            if managed.session.status == SessionStatus::Paused {
                let _ = managed.resume();
            }
        }
    }

    /// Kill all sessions.
    pub async fn kill_all(&mut self) {
        for managed in &mut self.sessions {
            if !managed.session.status.is_terminal() {
                let _ = managed.kill().await;
            }
        }
        self.save_state();
    }

    /// Check if all sessions are done.
    pub fn all_done(&self) -> bool {
        !self.sessions.is_empty() && self.sessions.iter().all(|s| s.session.status.is_terminal())
    }

    pub fn active_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| !s.session.status.is_terminal())
            .count()
    }

    fn push_log(&mut self, label: &str, message: &str) {
        self.activity_log.push(LogEntry {
            timestamp: Utc::now(),
            session_label: label.to_string(),
            message: message.to_string(),
        });
        // Keep last 200 entries
        if self.activity_log.len() > 200 {
            self.activity_log.drain(..self.activity_log.len() - 200);
        }
    }

    fn save_state(&mut self) {
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
