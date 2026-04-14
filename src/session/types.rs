use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

static ASCII_ICONS: AtomicBool = AtomicBool::new(false);

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
    NeedsPr,
    ConflictFix,
}

impl SessionStatus {
    pub fn nerd_symbol(&self) -> &'static str {
        match self {
            Self::Queued => "\u{f251}",       //  hourglass
            Self::Spawning => "\u{f46a}",     //  sync
            Self::Running => "\u{f40a}",      //  play
            Self::Completed => "\u{f42e}",    //  check_circle
            Self::GatesRunning => "\u{f422}", //  search
            Self::NeedsReview => "\u{f41b}",  //  issue_opened
            Self::Errored => "\u{f467}",      //  x_circle
            Self::Paused => "\u{f04c}",       //  pause
            Self::Killed => "\u{f2d3}",       //  skull
            Self::Stalled => "\u{f421}",      //  alert
            Self::Retrying => "\u{f363}",     //  refresh
            Self::CiFix => "\u{f7d9}",        //  wrench
            Self::NeedsPr => "\u{f407}",      //  git_pull_request
            Self::ConflictFix => "\u{f419}",  //  git_merge
        }
    }

    pub fn ascii_symbol(&self) -> &'static str {
        match self {
            Self::Queued => "[Q]",
            Self::Spawning => "[~]",
            Self::Running => "[>]",
            Self::Completed => "[+]",
            Self::GatesRunning => "[?]",
            Self::NeedsReview => "[!]",
            Self::Errored => "[X]",
            Self::Paused => "[-]",
            Self::Killed => "[x]",
            Self::Stalled => "[!]",
            Self::Retrying => "[R]",
            Self::CiFix => "[W]",
            Self::NeedsPr => "[P]",
            Self::ConflictFix => "[M]",
        }
    }

    pub fn symbol(&self) -> &'static str {
        if Self::use_nerd_font() {
            self.nerd_symbol()
        } else {
            self.ascii_symbol()
        }
    }

    /// Set icon mode globally. Called from config init and settings changes.
    pub fn set_ascii_icons(ascii: bool) {
        use std::sync::atomic::Ordering;
        ASCII_ICONS.store(ascii, Ordering::Relaxed);
    }

    fn use_nerd_font() -> bool {
        use std::sync::atomic::Ordering;
        !ASCII_ICONS.load(Ordering::Relaxed)
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
            Self::NeedsPr => "NEEDS_PR",
            Self::ConflictFix => "CONFLICT_FIX",
        }
    }

    /// Returns the set of valid target states from this state.
    pub fn valid_transitions(&self) -> &'static [SessionStatus] {
        use SessionStatus::*;
        match self {
            Queued => &[Spawning, Killed, CiFix, ConflictFix],
            Spawning => &[Running, Errored, Killed],
            Running => &[
                Completed,
                Errored,
                Paused,
                Stalled,
                Killed,
                GatesRunning,
                NeedsPr,
                CiFix,
                ConflictFix,
            ],
            Paused => &[Running, Killed],
            Stalled => &[Retrying, Killed, Errored],
            Completed => &[],
            GatesRunning => &[NeedsReview, Completed, Errored],
            NeedsReview => &[],
            Errored => &[Retrying],
            Retrying => &[Spawning, Errored, Killed],
            CiFix => &[Spawning, Errored, Killed],
            NeedsPr => &[Completed, Errored],
            ConflictFix => &[Spawning, Errored, Killed],
            Killed => &[],
        }
    }

    pub fn can_transition_to(&self, target: SessionStatus) -> bool {
        self.valid_transitions().contains(&target)
    }

    pub fn is_terminal(&self) -> bool {
        self.valid_transitions().is_empty()
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
pub struct ConflictFixContext {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub conflicting_files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_creation_tokens
    }

    /// Fraction of input that came from cache (0.0 to 1.0).
    pub fn cache_hit_ratio(&self) -> f64 {
        let total_input = self.input_tokens + self.cache_read_tokens;
        if total_input == 0 {
            return 0.0;
        }
        self.cache_read_tokens as f64 / total_input as f64
    }

    /// Fraction of total tokens that were output.
    pub fn output_ratio(&self) -> f64 {
        let total = self.total_tokens();
        if total == 0 {
            return 0.0;
        }
        self.output_tokens as f64 / total as f64
    }

    /// Cost per 1000 tokens, given a known total cost.
    pub fn cost_per_kilo_token(&self, cost_usd: f64) -> f64 {
        let total = self.total_tokens();
        if total == 0 {
            return 0.0;
        }
        cost_usd / (total as f64 / 1000.0)
    }

    /// Add another TokenUsage into this one (for aggregation across sessions).
    pub fn accumulate(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub status: SessionStatus,
    pub prompt: String,
    pub issue_number: Option<u64>,
    /// Additional issue numbers when this session handles multiple issues (unified PR).
    #[serde(default)]
    pub issue_numbers: Vec<u64>,
    pub model: String,
    pub mode: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub cost_usd: f64,
    pub context_pct: f64,
    /// Accumulated token usage from Claude CLI stream-json.
    #[serde(default)]
    pub token_usage: TokenUsage,
    pub current_activity: String,
    pub last_message: String,
    pub activity_log: Vec<ActivityEntry>,
    pub files_touched: Vec<String>,
    /// Previous file count for delta display in panels.
    #[serde(default)]
    pub files_touched_previous: usize,
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
    /// If this session is a conflict fix, tracks the PR and conflicting files.
    #[serde(default)]
    pub conflict_fix_context: Option<ConflictFixContext>,
    /// Image paths attached to this session for visual context.
    #[serde(default)]
    pub image_paths: Vec<PathBuf>,
    /// Gate results from the last gate check run (empty if gates not configured or not run yet).
    #[serde(default)]
    pub gate_results: Vec<GateResultEntry>,
    /// Whether this session completed without performing any observable work.
    #[serde(default)]
    pub is_hollow_completion: bool,
    /// Flash counter for visual transition effects. Decrements each render tick.
    #[serde(skip)]
    pub transition_flash_remaining: u8,
    /// Whether this session is currently in a thinking state.
    #[serde(skip)]
    pub is_thinking: bool,
    /// When the current thinking block started (for elapsed display).
    #[serde(skip)]
    pub thinking_started_at: Option<std::time::Instant>,
    /// History of state transitions for audit trail.
    #[serde(default)]
    pub transition_history: Vec<super::transition::SessionTransition>,
}

/// Lightweight gate result stored on a session for post-completion display.
/// Decoupled from `gates::types::GateResult` so session types don't depend on the gates module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResultEntry {
    pub gate: String,
    pub passed: bool,
    pub message: String,
}

#[allow(dead_code)] // Reason: gate result constructors — to be used by completion gates
impl GateResultEntry {
    pub fn pass(gate: &str, message: impl Into<String>) -> Self {
        Self {
            gate: gate.to_string(),
            passed: true,
            message: message.into(),
        }
    }

    pub fn fail(gate: &str, message: impl Into<String>) -> Self {
        Self {
            gate: gate.to_string(),
            passed: false,
            message: message.into(),
        }
    }
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
            issue_numbers: Vec::new(),
            model,
            mode,
            started_at: None,
            finished_at: None,
            cost_usd: 0.0,
            context_pct: 0.0,
            token_usage: TokenUsage::default(),
            current_activity: String::new(),
            last_message: String::new(),
            activity_log: Vec::new(),
            files_touched: Vec::new(),
            files_touched_previous: 0,
            pid: None,
            issue_title: None,
            retry_count: 0,
            last_retry_at: None,
            parent_session_id: None,
            child_session_ids: Vec::new(),
            fork_depth: 0,
            ci_fix_context: None,
            conflict_fix_context: None,
            image_paths: Vec::new(),
            gate_results: Vec::new(),
            is_hollow_completion: false,
            transition_flash_remaining: 0,
            is_thinking: false,
            thinking_started_at: None,
            transition_history: Vec::new(),
        }
    }

    /// Validated state transition. Records the transition in history.
    pub fn transition_to(
        &mut self,
        target: SessionStatus,
        reason: super::transition::TransitionReason,
    ) -> Result<(), super::transition::IllegalTransition> {
        if !self.status.can_transition_to(target) {
            return Err(super::transition::IllegalTransition {
                from: self.status,
                to: target,
            });
        }
        let from = self.status;
        let transition = super::transition::SessionTransition {
            from,
            to: target,
            reason,
            timestamp: Utc::now(),
        };
        self.status = target;
        self.transition_history.push(transition);

        // Visual transition flash (#202)
        self.transition_flash_remaining = 4;
        self.log_activity(format!(
            "STATUS: {} \u{2192} {}",
            from.label(),
            target.label()
        ));

        if target.is_terminal() && self.finished_at.is_none() {
            self.finished_at = Some(Utc::now());
        }

        Ok(())
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

    /// Threshold in seconds below which a zero-cost, zero-file session is suspicious.
    const HOLLOW_DURATION_THRESHOLD_SECS: i64 = 30;

    /// Check whether this session shows signs of a hollow completion —
    /// completed without spending money, touching files, or using tools.
    pub fn detect_hollow_completion(&self) -> bool {
        if self.cost_usd > 0.0 {
            return false;
        }
        if !self.files_touched.is_empty() {
            return false;
        }
        if self.has_tool_calls() {
            return false;
        }
        let duration_secs = self.elapsed().map(|d| d.num_seconds()).unwrap_or(i64::MAX);
        duration_secs < Self::HOLLOW_DURATION_THRESHOLD_SECS
    }

    /// Whether the activity log contains any tool-use entries.
    pub fn has_tool_calls(&self) -> bool {
        self.activity_log.iter().any(|e| {
            e.message.starts_with("Tool ")
                || e.message.starts_with("Tool:")
                || e.message.starts_with("Bash: ")
                || e.message.starts_with("Read: ")
                || e.message.starts_with("Edit: ")
                || e.message.starts_with("Write: ")
                || e.message.starts_with("Glob: ")
                || e.message.starts_with("Grep: ")
        })
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
        /// Extracted file path, if this is a file-touching tool.
        file_path: Option<String>,
        /// Extracted command for Bash tool (first ~60 chars).
        command_preview: Option<String>,
    },
    /// Tool result received
    ToolResult { tool: String, is_error: bool },
    /// Cost update from usage data
    #[allow(dead_code)] // Reason: cost tracking event — to be emitted by budget enforcer
    CostUpdate { cost_usd: f64 },
    /// Session completed
    Completed { cost_usd: f64 },
    /// Error occurred
    Error { message: String },
    /// Context window usage update
    ContextUpdate { context_pct: f64 },
    /// Token usage update from usage data in stream-json
    TokenUpdate { usage: TokenUsage },
    /// Assistant is thinking (extended thinking block)
    Thinking { text: String },
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

    // --- Issue #134: Thinking state fields ---

    #[test]
    fn session_is_thinking_defaults_to_false() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        assert!(!s.is_thinking);
        assert!(s.thinking_started_at.is_none());
    }

    #[test]
    fn session_thinking_fields_skipped_in_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.is_thinking = true;
        s.thinking_started_at = Some(std::time::Instant::now());

        let json = serde_json::to_string(&s).unwrap();
        // The skipped fields should not appear in JSON
        assert!(!json.contains("is_thinking"));
        assert!(!json.contains("thinking_started_at"));

        // Deserialize should default them
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert!(!rt.is_thinking);
        assert!(rt.thinking_started_at.is_none());
    }

    // --- Issue #102: Enhanced real-time session activity feedback ---

    // --- Issue #159: NeedsPr status tests ---

    #[test]
    fn needs_pr_status_is_not_terminal() {
        assert!(!SessionStatus::NeedsPr.is_terminal());
    }

    #[test]
    fn needs_pr_status_has_symbol_and_label() {
        let status = SessionStatus::NeedsPr;
        assert!(!status.symbol().is_empty());
        assert_eq!(status.label(), "NEEDS_PR");
    }

    #[test]
    fn needs_pr_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::NeedsPr).unwrap();
        assert_eq!(json, r#""needs_pr""#);
    }

    #[test]
    fn needs_pr_status_deserializes_from_snake_case() {
        let status: SessionStatus = serde_json::from_str(r#""needs_pr""#).unwrap();
        assert_eq!(status, SessionStatus::NeedsPr);
    }

    #[test]
    fn stream_event_thinking_variant_holds_text() {
        let e = StreamEvent::Thinking {
            text: "reasoning".to_string(),
        };
        match e {
            StreamEvent::Thinking { text } => assert_eq!(text, "reasoning"),
            other => panic!("Expected Thinking, got {:?}", other),
        }
    }

    #[test]
    fn stream_event_tool_use_command_preview_is_none_for_read() {
        let e = StreamEvent::ToolUse {
            tool: "Read".to_string(),

            file_path: Some("/src/main.rs".to_string()),
            command_preview: None,
        };
        match e {
            StreamEvent::ToolUse {
                command_preview, ..
            } => assert_eq!(command_preview, None),
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn stream_event_tool_use_command_preview_holds_value() {
        let e = StreamEvent::ToolUse {
            tool: "Bash".to_string(),

            file_path: None,
            command_preview: Some("cargo build".to_string()),
        };
        match e {
            StreamEvent::ToolUse {
                command_preview, ..
            } => {
                assert_eq!(command_preview, Some("cargo build".to_string()))
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
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

    // --- Issue #104: Session::gate_results field ---

    #[test]
    fn session_gate_results_defaults_to_empty() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        assert!(s.gate_results.is_empty());
    }

    #[test]
    fn session_gate_results_round_trips_via_serde() {
        let mut s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
        );
        s.gate_results = vec![
            GateResultEntry::pass("tests", "all passed"),
            GateResultEntry::fail("clippy", "2 warnings"),
        ];
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.gate_results.len(), 2);
        assert!(rt.gate_results[0].passed);
        assert!(!rt.gate_results[1].passed);
    }

    #[test]
    fn session_gate_results_deserializes_with_default_when_field_absent() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","gate_results":[]"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.gate_results.is_empty());
    }

    // --- Issue #140: SessionStatus::ConflictFix and ConflictFixContext ---

    #[test]
    fn conflict_fix_status_is_not_terminal() {
        assert!(!SessionStatus::ConflictFix.is_terminal());
    }

    #[test]
    fn conflict_fix_status_has_non_empty_symbol() {
        assert!(!SessionStatus::ConflictFix.symbol().is_empty());
    }

    #[test]
    fn conflict_fix_status_has_correct_label() {
        assert_eq!(SessionStatus::ConflictFix.label(), "CONFLICT_FIX");
    }

    #[test]
    fn conflict_fix_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::ConflictFix).unwrap();
        assert_eq!(json, r#""conflict_fix""#);
    }

    #[test]
    fn conflict_fix_status_deserializes_from_snake_case() {
        let status: SessionStatus = serde_json::from_str(r#""conflict_fix""#).unwrap();
        assert_eq!(status, SessionStatus::ConflictFix);
    }

    #[test]
    fn conflict_fix_context_stores_all_fields() {
        let ctx = ConflictFixContext {
            pr_number: 42,
            issue_number: 10,
            branch: "feat/fix".to_string(),
            conflicting_files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
        };
        assert_eq!(ctx.pr_number, 42);
        assert_eq!(ctx.issue_number, 10);
        assert_eq!(ctx.branch, "feat/fix");
        assert_eq!(ctx.conflicting_files.len(), 2);
    }

    #[test]
    fn conflict_fix_context_conflicting_files_is_a_vec() {
        let ctx = ConflictFixContext {
            pr_number: 1,
            issue_number: 1,
            branch: "b".to_string(),
            conflicting_files: vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()],
        };
        assert_eq!(ctx.conflicting_files, vec!["a.rs", "b.rs", "c.rs"]);
    }

    #[test]
    fn session_conflict_fix_context_defaults_to_none() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        assert!(s.conflict_fix_context.is_none());
    }

    #[test]
    fn session_with_conflict_fix_context_round_trips_via_serde() {
        let mut s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
        );
        s.conflict_fix_context = Some(ConflictFixContext {
            pr_number: 99,
            issue_number: 42,
            branch: "feat/merge-fix".into(),
            conflicting_files: vec!["src/main.rs".into(), "src/lib.rs".into()],
        });
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        let ctx = rt.conflict_fix_context.unwrap();
        assert_eq!(ctx.pr_number, 99);
        assert_eq!(ctx.issue_number, 42);
        assert_eq!(ctx.branch, "feat/merge-fix");
        assert_eq!(ctx.conflicting_files.len(), 2);
    }

    #[test]
    fn session_conflict_fix_context_deserializes_with_default_when_field_absent() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","conflict_fix_context":null"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.conflict_fix_context.is_none());
    }

    // --- Issue #169: Hollow completion detection ---

    fn make_session_for_hollow() -> Session {
        Session::new(
            "test prompt".into(),
            "claude-sonnet-4-6".into(),
            "orchestrator".into(),
            None,
        )
    }

    fn with_timestamps(mut s: Session, started_secs_ago: i64) -> Session {
        let now = Utc::now();
        s.started_at = Some(now - chrono::Duration::seconds(started_secs_ago));
        s.finished_at = Some(now);
        s
    }

    #[test]
    fn has_tool_calls_returns_false_when_activity_log_is_empty() {
        let s = make_session_for_hollow();
        assert!(!s.has_tool_calls());
    }

    #[test]
    fn has_tool_calls_returns_true_when_log_contains_tool_prefix() {
        let mut s = make_session_for_hollow();
        s.log_activity("Tool: WebSearch".into());
        assert!(s.has_tool_calls());
    }

    #[test]
    fn has_tool_calls_returns_true_when_log_contains_bash_prefix() {
        let mut s = make_session_for_hollow();
        s.log_activity("Bash: $ cargo build".into());
        assert!(s.has_tool_calls());
    }

    #[test]
    fn has_tool_calls_returns_false_for_non_tool_activity_entries() {
        let mut s = make_session_for_hollow();
        s.log_activity("Session spawned (pid: 1234)".into());
        s.log_activity("Context: 12%".into());
        s.log_activity("Session completed".into());
        assert!(!s.has_tool_calls());
    }

    #[test]
    fn has_tool_calls_returns_true_for_file_path_tool_entry() {
        let mut s = make_session_for_hollow();
        s.log_activity("Read: src/main.rs".into());
        assert!(s.has_tool_calls());
    }

    #[test]
    fn detect_hollow_completion_returns_true_for_all_hollow_conditions_met() {
        let mut s = with_timestamps(make_session_for_hollow(), 10);
        s.cost_usd = 0.0;
        s.log_activity("Session spawned (pid: 1)".into());
        s.log_activity("Session completed".into());
        assert!(s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_false_when_cost_is_nonzero() {
        let mut s = with_timestamps(make_session_for_hollow(), 10);
        s.cost_usd = 0.05;
        assert!(!s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_false_when_files_touched_is_nonempty() {
        let mut s = with_timestamps(make_session_for_hollow(), 10);
        s.cost_usd = 0.0;
        s.files_touched = vec!["src/main.rs".into()];
        assert!(!s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_false_when_activity_log_has_tool_calls() {
        let mut s = with_timestamps(make_session_for_hollow(), 10);
        s.cost_usd = 0.0;
        s.log_activity("Bash: $ echo hi".into());
        assert!(!s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_false_when_duration_is_exactly_30s() {
        let mut s = with_timestamps(make_session_for_hollow(), 30);
        s.cost_usd = 0.0;
        assert!(!s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_true_at_duration_just_below_30s() {
        let mut s = with_timestamps(make_session_for_hollow(), 29);
        s.cost_usd = 0.0;
        assert!(s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_false_when_started_at_is_none() {
        let mut s = make_session_for_hollow();
        s.cost_usd = 0.0;
        s.started_at = None;
        s.finished_at = None;
        assert!(!s.detect_hollow_completion());
    }

    #[test]
    fn detect_hollow_completion_returns_false_for_long_zero_cost_session() {
        let mut s = with_timestamps(make_session_for_hollow(), 120);
        s.cost_usd = 0.0;
        assert!(!s.detect_hollow_completion());
    }

    #[test]
    fn is_hollow_completion_field_round_trips_via_serde() {
        let mut s = make_session_for_hollow();
        s.is_hollow_completion = true;
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert!(rt.is_hollow_completion);
    }

    // --- Issue #161: TokenUsage tests ---

    #[test]
    fn token_usage_default_is_all_zeros() {
        let t = TokenUsage::default();
        assert_eq!(t.input_tokens, 0);
        assert_eq!(t.output_tokens, 0);
        assert_eq!(t.cache_read_tokens, 0);
        assert_eq!(t.cache_creation_tokens, 0);
    }

    #[test]
    fn token_usage_total_tokens() {
        let t = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 30,
            cache_creation_tokens: 20,
        };
        assert_eq!(t.total_tokens(), 200);
    }

    #[test]
    fn token_usage_cache_hit_ratio_zero_when_no_input() {
        let t = TokenUsage::default();
        assert!((t.cache_hit_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn token_usage_cache_hit_ratio_computes_correctly() {
        let t = TokenUsage {
            input_tokens: 25000,
            output_tokens: 1000,
            cache_read_tokens: 45000,
            cache_creation_tokens: 0,
        };
        // cache_read / (input + cache_read) = 45000 / 70000
        let expected = 45000.0 / 70000.0;
        assert!((t.cache_hit_ratio() - expected).abs() < 0.001);
    }

    #[test]
    fn token_usage_output_ratio_zero_when_empty() {
        let t = TokenUsage::default();
        assert!((t.output_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn token_usage_cost_per_kilo_token() {
        let t = TokenUsage {
            input_tokens: 10000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };
        // $1.00 / (10000/1000) = $0.10 per kTok
        let cpk = t.cost_per_kilo_token(1.0);
        assert!((cpk - 0.1).abs() < 0.001);
    }

    #[test]
    fn token_usage_accumulate_adds_fields() {
        let mut a = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 30,
            cache_creation_tokens: 20,
        };
        let b = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_tokens: 60,
            cache_creation_tokens: 40,
        };
        a.accumulate(&b);
        assert_eq!(a.input_tokens, 300);
        assert_eq!(a.output_tokens, 150);
        assert_eq!(a.cache_read_tokens, 90);
        assert_eq!(a.cache_creation_tokens, 60);
    }

    #[test]
    fn token_usage_round_trips_via_serde() {
        let t = TokenUsage {
            input_tokens: 42000,
            output_tokens: 1500,
            cache_read_tokens: 30000,
            cache_creation_tokens: 2000,
        };
        let json = serde_json::to_string(&t).unwrap();
        let rt: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.input_tokens, 42000);
        assert_eq!(rt.output_tokens, 1500);
        assert_eq!(rt.cache_read_tokens, 30000);
        assert_eq!(rt.cache_creation_tokens, 2000);
    }

    #[test]
    fn session_token_usage_defaults_when_absent_in_json() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        let json = serde_json::to_string(&s).unwrap();
        // Strip the token_usage field to simulate old JSON
        let stripped = json.replace(
            r#","token_usage":{"input_tokens":0,"output_tokens":0,"cache_read_tokens":0,"cache_creation_tokens":0}"#,
            "",
        );
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert_eq!(rt.token_usage.total_tokens(), 0);
    }

    #[test]
    fn is_hollow_completion_defaults_to_false_when_absent_in_json() {
        let s = make_session_for_hollow();
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(",\"is_hollow_completion\":false", "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(!rt.is_hollow_completion);
    }

    // --- Issue #202: Transition flash effects ---

    #[test]
    fn flash_counter_starts_at_zero() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        assert_eq!(s.transition_flash_remaining, 0);
    }

    #[test]
    fn transition_to_sets_flash_counter() {
        use crate::session::transition::TransitionReason;
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        assert_eq!(s.transition_flash_remaining, 4);
    }

    #[test]
    fn transition_to_resets_flash_counter_on_each_transition() {
        use crate::session::transition::TransitionReason;
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        s.transition_flash_remaining = 1; // simulate partial decay
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        assert_eq!(s.transition_flash_remaining, 4);
    }

    #[test]
    fn failed_transition_does_not_set_flash_counter() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        // Queued -> Completed is invalid
        let _ = s.transition_to(
            SessionStatus::Completed,
            crate::session::transition::TransitionReason::StreamCompleted,
        );
        assert_eq!(s.transition_flash_remaining, 0);
    }

    #[test]
    fn transition_to_logs_status_change_in_activity_log() {
        use crate::session::transition::TransitionReason;
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        let last = s
            .activity_log
            .last()
            .expect("activity log should have entry");
        assert!(
            last.message.contains("STATUS:"),
            "expected STATUS: prefix, got: {}",
            last.message
        );
        assert!(
            last.message.contains("QUEUED"),
            "expected QUEUED in message, got: {}",
            last.message
        );
        assert!(
            last.message.contains("SPAWNING"),
            "expected SPAWNING in message, got: {}",
            last.message
        );
    }

    #[test]
    fn flash_counter_skipped_in_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.transition_flash_remaining = 4;
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("transition_flash_remaining"));
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.transition_flash_remaining, 0);
    }
}
