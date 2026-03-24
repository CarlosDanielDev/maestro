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
    /// Raw line we couldn't parse
    Unknown { raw: String },
}
