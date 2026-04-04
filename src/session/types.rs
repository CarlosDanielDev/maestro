use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Queued,
    Spawning,
    Running,
    Completed,
    GatesRunning,
    NeedsReview,
    Errored,
    Paused,
    Killed,
    Stalled,
    Retrying,
    CiFix,
}

impl SessionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Queued => "⏳",
            Self::Spawning => "🔄",
            Self::Running => "▶",
            Self::Completed => "✅",
            Self::GatesRunning => "🔍",
            Self::NeedsReview => "⚡",
            Self::Errored => "❌",
            Self::Paused => "⏸",
            Self::Killed => "💀",
            Self::Stalled => "⚠",
            Self::Retrying => "🔁",
            Self::CiFix => "🔧",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Spawning => "SPAWNING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::GatesRunning => "GATES_RUNNING",
            Self::NeedsReview => "NEEDS_REVIEW",
            Self::Errored => "ERRORED",
            Self::Paused => "PAUSED",
            Self::Killed => "KILLED",
            Self::Stalled => "STALLED",
            Self::Retrying => "RETRYING",
            Self::CiFix => "CI_FIX",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Errored | Self::Killed | Self::NeedsReview
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiFixContext {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub attempt: u32,
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
    /// If this session is a CI fix, tracks the PR and attempt number.
    #[serde(default)]
    pub ci_fix_context: Option<CiFixContext>,
    /// Image paths attached to this session for visual context.
    #[serde(default)]
    pub image_paths: Vec<PathBuf>,
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
            ci_fix_context: None,
            image_paths: Vec::new(),
        }
    }

    /// Builder method to attach image paths to a session.
    pub fn with_image_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.image_paths = paths;
        self
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
    fn needs_review_status_is_terminal() {
        assert!(SessionStatus::NeedsReview.is_terminal());
    }

    #[test]
    fn gates_running_status_is_not_terminal() {
        assert!(!SessionStatus::GatesRunning.is_terminal());
    }

    #[test]
    fn gates_running_has_symbol_and_label() {
        let status = SessionStatus::GatesRunning;
        assert!(!status.symbol().is_empty());
        assert_eq!(status.label(), "GATES_RUNNING");
    }

    #[test]
    fn needs_review_has_symbol_and_label() {
        let status = SessionStatus::NeedsReview;
        assert!(!status.symbol().is_empty());
        assert_eq!(status.label(), "NEEDS_REVIEW");
    }

    #[test]
    fn session_status_gates_running_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::GatesRunning).unwrap();
        assert_eq!(json, r#""gates_running""#);
    }

    #[test]
    fn session_status_needs_review_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::NeedsReview).unwrap();
        assert_eq!(json, r#""needs_review""#);
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

    #[test]
    fn ci_fix_status_is_not_terminal() {
        assert!(!SessionStatus::CiFix.is_terminal());
    }

    #[test]
    fn ci_fix_status_has_symbol_and_label() {
        let status = SessionStatus::CiFix;
        assert!(!status.symbol().is_empty());
        assert_eq!(status.label(), "CI_FIX");
    }

    #[test]
    fn ci_fix_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::CiFix).unwrap();
        assert_eq!(json, r#""ci_fix""#);
    }

    #[test]
    fn ci_fix_context_stores_all_fields() {
        let ctx = CiFixContext {
            pr_number: 99,
            issue_number: 42,
            branch: "feat/fix".into(),
            attempt: 1,
        };
        assert_eq!(ctx.pr_number, 99);
        assert_eq!(ctx.issue_number, 42);
        assert_eq!(ctx.branch, "feat/fix");
        assert_eq!(ctx.attempt, 1);
    }

    #[test]
    fn session_ci_fix_context_defaults_to_none() {
        let s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(10),
        );
        assert!(s.ci_fix_context.is_none());
    }

    // --- image_paths field tests (issue #42) ---

    #[test]
    fn session_new_initializes_image_paths_as_empty() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        assert!(s.image_paths.is_empty());
    }

    #[test]
    fn session_with_image_paths_builder() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None)
            .with_image_paths(vec![
                std::path::PathBuf::from("/tmp/a.png"),
                std::path::PathBuf::from("/tmp/b.jpg"),
            ]);
        assert_eq!(s.image_paths.len(), 2);
    }

    #[test]
    fn session_with_image_paths_round_trips_via_serde() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None)
            .with_image_paths(vec![
                std::path::PathBuf::from("img/a.png"),
                std::path::PathBuf::from("img/b.jpg"),
            ]);
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.image_paths, s.image_paths);
    }

    #[test]
    fn session_image_paths_deserializes_with_default_when_field_absent() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","image_paths":[]"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.image_paths.is_empty());
    }

    #[test]
    fn session_with_ci_fix_context_round_trips_via_serde() {
        let mut s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
        );
        s.ci_fix_context = Some(CiFixContext {
            pr_number: 5,
            issue_number: 1,
            branch: "feat/fix".into(),
            attempt: 2,
        });
        let json = serde_json::to_string(&s).unwrap();
        let round_tripped: Session = serde_json::from_str(&json).unwrap();
        let ctx = round_tripped.ci_fix_context.unwrap();
        assert_eq!(ctx.attempt, 2);
        assert_eq!(ctx.pr_number, 5);
    }
}
