use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Queued,
    Spawning,
    Running,
    Completed,
    Errored,
    Paused,
    Killed,
    Stalled,
    Retrying,
}

impl SessionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Queued => "⏳",
            Self::Spawning => "🔄",
            Self::Running => "▶",
            Self::Completed => "✅",
            Self::Errored => "❌",
            Self::Paused => "⏸",
            Self::Killed => "💀",
            Self::Stalled => "⚠",
            Self::Retrying => "🔁",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Spawning => "SPAWNING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Errored => "ERRORED",
            Self::Paused => "PAUSED",
            Self::Killed => "KILLED",
            Self::Stalled => "STALLED",
            Self::Retrying => "RETRYING",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Errored | Self::Killed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub status: SessionStatus,
    pub prompt: String,
    pub issue_number: Option<u64>,
    pub model: String,
    pub mode: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub cost_usd: f64,
    pub context_pct: f64,
    pub current_activity: String,
    pub last_message: String,
    pub activity_log: Vec<ActivityEntry>,
    pub files_touched: Vec<String>,
    pub pid: Option<u32>,
    /// Issue title for display in TUI panels.
    #[serde(default)]
    pub issue_title: Option<String>,
    /// Number of times this session has been retried.
    #[serde(default)]
    pub retry_count: u32,
    /// Timestamp of the last retry attempt.
    #[serde(default)]
    pub last_retry_at: Option<DateTime<Utc>>,
    /// Parent session ID if this is a forked continuation.
    #[serde(default)]
    pub parent_session_id: Option<Uuid>,
    /// Child session IDs if this session was forked.
    #[serde(default)]
    pub child_session_ids: Vec<Uuid>,
    /// Fork depth in the chain (0 = original, 1 = first fork, etc.)
    #[serde(default)]
    pub fork_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

impl Session {
    pub fn new(prompt: String, model: String, mode: String, issue_number: Option<u64>) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: SessionStatus::Queued,
            prompt,
            issue_number,
            model,
            mode,
            started_at: None,
            finished_at: None,
            cost_usd: 0.0,
            context_pct: 0.0,
            current_activity: String::new(),
            last_message: String::new(),
            activity_log: Vec::new(),
            files_touched: Vec::new(),
            pid: None,
            issue_title: None,
            retry_count: 0,
            last_retry_at: None,
            parent_session_id: None,
            child_session_ids: Vec::new(),
            fork_depth: 0,
        }
    }

    pub fn log_activity(&mut self, message: String) {
        self.activity_log.push(ActivityEntry {
            timestamp: Utc::now(),
            message,
        });
        // Keep last 100 entries
        if self.activity_log.len() > 100 {
            self.activity_log.drain(..self.activity_log.len() - 100);
        }
    }

    pub fn elapsed(&self) -> Option<chrono::Duration> {
        self.started_at.map(|start| {
            let end = self.finished_at.unwrap_or_else(Utc::now);
            end - start
        })
    }

    pub fn elapsed_display(&self) -> String {
        match self.elapsed() {
            Some(d) => {
                let secs = d.num_seconds();
                let mins = secs / 60;
                let secs = secs % 60;
                if mins > 0 {
                    format!("{}m{:02}s", mins, secs)
                } else {
                    format!("{}s", secs)
                }
            }
            None => "—".into(),
        }
    }
}

/// Events emitted by the Claude CLI JSON stream that we care about.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Assistant started producing a message
    AssistantMessage { text: String },
    /// A tool is being used
    ToolUse {
        tool: String,
        args_preview: String,
        /// Extracted file path, if this is a file-touching tool.
        file_path: Option<String>,
    },
    /// Tool result received
    ToolResult { tool: String, is_error: bool },
    /// Cost update from usage data
    CostUpdate { cost_usd: f64 },
    /// Session completed
    Completed { cost_usd: f64 },
    /// Error occurred
    Error { message: String },
    /// Context window usage update
    ContextUpdate { context_pct: f64 },
    /// Raw line we couldn't parse
    Unknown { raw: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_new_initializes_fork_fields_to_defaults() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        assert_eq!(s.parent_session_id, None);
        assert!(s.child_session_ids.is_empty());
        assert_eq!(s.fork_depth, 0);
    }

    #[test]
    fn stream_event_context_update_holds_value() {
        let event = StreamEvent::ContextUpdate { context_pct: 72.5 };
        match event {
            StreamEvent::ContextUpdate { context_pct } => {
                assert!((context_pct - 72.5).abs() < f64::EPSILON);
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }
}
