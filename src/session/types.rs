use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Provenance of a session — gates runtime features that depend on whether
/// the session was started by the user directly or dispatched by the
/// orchestration layers.
///
/// `DirectUser` is the default for backward compatibility and matches the
/// behavior of every existing `Session::new` call site. L1/L2 dispatch paths
/// do NOT flow through `SessionPool`/`ManagedSession` today (see
/// `src/orchestration/dispatch.rs`); the `OrchestratorL1` / `OrchestratorL2`
/// variants exist to make the gate in `pool.rs` explicit and to give future
/// orchestration refactors a typed escape hatch. See issue #707.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOrigin {
    #[default]
    DirectUser,
    OrchestratorL1,
    OrchestratorL2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Queued,
    Spawning,
    Running,
    Completed,
    GatesRunning,
    NeedsReview,
    /// Terminal status used when post-completion gates fail. Distinct from
    /// `Errored` so the dispatcher knows to retain the worktree for recovery
    /// rather than tear it down with the rest of the session state.
    FailedGates,
    Errored,
    Paused,
    Killed,
    Stalled,
    Retrying,
    CiFix,
    NeedsPr,
    ConflictFix,
    /// Kept-alive state for `SessionMode::Interactive` sessions (#948):
    /// the one-shot flow settled (see `Session.settled_from`) and the
    /// session now accepts follow-up turns on the same resume id.
    /// NOT terminal — only an explicit quit/kill ends it.
    Interactive,
}

/// How a session is driven: a single one-shot run, or a long-lived
/// interactive conversation (#734). Derived from the launch-dialog
/// `interaction` flag (#733) via `From<bool>`. Distinct from
/// `crate::provider::types::SessionMode`, which is the label-derived
/// agent mode (orchestrator / vibe) — different concept, different module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    #[default]
    OneShot,
    Interactive,
}

impl From<bool> for SessionMode {
    /// `interaction == true` → `Interactive`, otherwise `OneShot`.
    fn from(interaction: bool) -> Self {
        if interaction {
            SessionMode::Interactive
        } else {
            SessionMode::OneShot
        }
    }
}

impl SessionStatus {
    pub const fn nerd_symbol(&self) -> &'static str {
        crate::icons::get_for_mode(self.icon_id(), true)
    }

    /// Maps each status to its icon registry entry.
    pub const fn icon_id(&self) -> crate::icons::IconId {
        use crate::icons::IconId;
        match self {
            Self::Queued => IconId::Hourglass,
            Self::Spawning => IconId::Sync,
            Self::Running => IconId::Play,
            Self::Completed => IconId::CheckCircle,
            Self::GatesRunning => IconId::Search,
            Self::NeedsReview => IconId::NeedsReview,
            Self::FailedGates => IconId::XCircle,
            Self::Errored => IconId::XCircle,
            Self::Paused => IconId::Pause,
            Self::Killed => IconId::Skull,
            Self::Stalled => IconId::Alert,
            Self::Retrying => IconId::Refresh,
            Self::CiFix => IconId::Wrench,
            Self::NeedsPr => IconId::GitPr,
            Self::ConflictFix => IconId::GitMerge,
            Self::Interactive => IconId::Play,
        }
    }

    pub const fn ascii_symbol(&self) -> &'static str {
        match self {
            Self::Queued => "[Q]",
            Self::Spawning => "[~]",
            Self::Running => "[>]",
            Self::Completed => "[+]",
            Self::GatesRunning => "[?]",
            Self::NeedsReview => "[!]",
            Self::FailedGates => "[!G]",
            Self::Errored => "[X]",
            Self::Paused => "[-]",
            Self::Killed => "[x]",
            Self::Stalled => "[!]",
            Self::Retrying => "[R]",
            Self::CiFix => "[W]",
            Self::NeedsPr => "[P]",
            Self::ConflictFix => "[M]",
            Self::Interactive => "[I]",
        }
    }

    pub fn symbol(&self) -> &'static str {
        if crate::icon_mode::use_nerd_font() {
            self.nerd_symbol()
        } else {
            self.ascii_symbol()
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Spawning => "SPAWNING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::GatesRunning => "GATES_RUNNING",
            Self::NeedsReview => "NEEDS_REVIEW",
            Self::FailedGates => "FAILED_GATES",
            Self::Errored => "ERRORED",
            Self::Paused => "PAUSED",
            Self::Killed => "KILLED",
            Self::Stalled => "STALLED",
            Self::Retrying => "RETRYING",
            Self::CiFix => "CI_FIX",
            Self::NeedsPr => "NEEDS_PR",
            Self::ConflictFix => "CONFLICT_FIX",
            Self::Interactive => "INTERACTIVE",
        }
    }

    /// Returns the set of valid target states from this state.
    pub const fn valid_transitions(&self) -> &'static [SessionStatus] {
        use SessionStatus::*;
        match self {
            Queued => &[Spawning, Killed, CiFix, ConflictFix],
            // `Interactive` appears as a target wherever a settleable
            // terminal ({Completed, FailedGates, NeedsPr, Errored}) does:
            // the #948 interception in `Session::transition_to` rewrites
            // those targets to `Interactive` for interactive-mode sessions,
            // so the rewritten transition must validate from the same
            // source statuses. One-shot sessions never request it.
            Spawning => &[Running, Errored, Killed, Interactive],
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
                Interactive,
            ],
            Paused => &[Running, Killed],
            Stalled => &[Retrying, Killed, Errored, Interactive],
            Completed => &[],
            GatesRunning => &[NeedsReview, FailedGates, Completed, Errored, Interactive],
            NeedsReview => &[],
            FailedGates => &[],
            Errored => &[Retrying],
            Retrying => &[Spawning, Errored, Killed, Interactive],
            CiFix => &[Spawning, Errored, Killed, Interactive],
            NeedsPr => &[Completed, Errored, Interactive],
            ConflictFix => &[Spawning, Errored, Killed, Interactive],
            Killed => &[],
            // Kept alive until explicit quit/kill; re-settles onto itself
            // after each follow-up turn (settled_from updates).
            Interactive => &[Interactive, Killed],
        }
    }

    pub fn can_transition_to(&self, target: SessionStatus) -> bool {
        self.valid_transitions().contains(&target)
    }

    pub const fn is_terminal(&self) -> bool {
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

/// Per-field upper bound for token counts emitted in a single frame.
///
/// Values above this cap are treated as upstream corruption (malicious,
/// MITM'd, or buggy providers): each parser clamps to this value and
/// emits a `StreamEvent::Warning` with code `"token_count_clamped"` so
/// operators see the misbehavior. Chosen at 100M because the largest
/// shipped context windows are ~10M; 10× headroom leaves room for
/// legitimate growth while still bounding `f64` cost arithmetic well
/// inside finite range.
pub const TOKEN_COUNT_CAP: u64 = 100_000_000;

/// Clamp a raw upstream token count to [`TOKEN_COUNT_CAP`].
///
/// Callers must emit a `StreamEvent::Warning` whenever the return value
/// differs from the input — see the per-parser `sanitize_*` helpers in
/// `src/session/parser.rs`, `src/agent_provider/codex/parser.rs`,
/// `src/agent_provider/opencode/parser.rs`, and
/// `src/agent_provider/openai_compat/sse.rs`.
pub fn sanitize_token_count(raw: u64) -> u64 {
    raw.min(TOKEN_COUNT_CAP)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

impl TokenUsage {
    pub const fn total_tokens(&self) -> u64 {
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
    pub const fn accumulate(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
    }
}

/// Resolved per-session mode settings.
///
/// Kept in `session::types` rather than `config` so sessions can persist the
/// effective spawn-time settings without depending on the full runtime config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionModeConfig {
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
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
    /// How this session is driven (#947): one-shot run vs. kept-alive
    /// interactive conversation. `serde(default)` (= `OneShot`) so
    /// pre-#947 state files load unchanged. Distinct from `mode`, which
    /// is the agent mode label (orchestrator / vibe).
    #[serde(default)]
    pub session_mode: SessionMode,
    /// Provider-side conversation id (#947) — the id `--resume <id>`
    /// takes. Bound once from the first run result via
    /// [`Session::bind_agent_session_id`]; never set directly.
    #[serde(default)]
    pub agent_session_id: Option<String>,
    /// The one-shot outcome an Interactive session settled from (#948):
    /// `Completed` / `FailedGates` / `NeedsPr` / `Errored`. Set by the
    /// `transition_to` interception; shown in the kept-alive banner.
    /// Always `None` for one-shot sessions.
    #[serde(default)]
    pub settled_from: Option<SessionStatus>,
    /// Turn-level activity inside the kept-alive state (#948) — drives
    /// the chat input lock. In-memory only.
    #[serde(skip)]
    pub turn_state: super::interaction::TurnState,
    /// Chat transcript for Interactive sessions (#948) — the `Session`
    /// owns the turns (was `InteractionSession.history`). Persisted so
    /// re-entry survives a restart. Empty for one-shot sessions.
    #[serde(default)]
    pub turns: Vec<super::interaction::TurnRecord>,
    /// Launch-time "Produce PR" choice for Interactive sessions (#948,
    /// was `InteractionSession.produce_pr`). Gates the Ctrl+P pushup
    /// chord on the Interaction screen.
    #[serde(default)]
    pub produce_pr: bool,
    /// `/pushup` PR linked to this interactive session's issue (#949,
    /// spec §4.4). Set by `ManagedSession::signal_pr_linked`; the session
    /// stays alive — teardown happens only on explicit quit. Carries the
    /// PR number for the System-turn announcement and the banner.
    #[serde(default)]
    pub pr_linked: Option<u64>,
    /// Configured agent id selected when this session was created.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Resolved mode settings captured when the session was created.
    #[serde(default)]
    pub mode_config: Option<SessionModeConfig>,
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
    /// User dismissed the hollow-completion recovery modal (`[s]` Skip or
    /// Esc) for this session. In-memory only — re-firing after a restart
    /// is acceptable. Prevents the completion pipeline from re-opening the
    /// modal on every tick after the user explicitly skipped (#890).
    #[serde(skip)]
    pub hollow_dismissed: bool,
    /// Flash counter for visual transition effects. Decrements each render tick.
    #[serde(skip)]
    pub transition_flash_remaining: u8,
    /// Whether this session is currently in a thinking state.
    #[serde(skip)]
    pub is_thinking: bool,
    /// When the current thinking block started (for elapsed display).
    #[serde(skip)]
    pub thinking_started_at: Option<std::time::Instant>,
    /// TurboQuant: original token count of a fork-handoff compression (real savings).
    /// Populated when this session was the parent of a fork that used TurboQuant
    /// to compress the handed-off context (#343). `None` means no real savings
    /// data — the dashboard falls back to a projection.
    #[serde(default)]
    pub tq_handoff_original_tokens: Option<u64>,
    /// TurboQuant: compressed token count of a fork-handoff compression.
    #[serde(default)]
    pub tq_handoff_compressed_tokens: Option<u64>,
    /// Retained worktree for `FailedGates` recovery. `None` when no
    /// worktree survived completion.
    #[serde(default)]
    pub worktree_path: Option<PathBuf>,
    /// History of state transitions for audit trail.
    #[serde(default)]
    pub transition_history: Vec<super::transition::SessionTransition>,
    /// Classified intent of the prompt (Work vs. Consultation). Derived at spawn time.
    #[serde(default)]
    pub intent: super::intent::SessionIntent,
    /// Role classification for the agent. Stored explicitly so the agent-graph
    /// renderer has an O(1) lookup and so a `--role` override survives across
    /// resume/restart. Set once at `Session::new` and frozen for the session
    /// lifetime; `transition_to` does NOT mutate this field. See
    /// `docs/adr/002-agent-personalities.md` § Data Model.
    #[serde(default)]
    pub role: super::role::Role,
    /// Persisted stream events for the per-agent call-log viewer (#868).
    /// Capped at [`Session::CALL_LOG_CAP`]; oldest entries are dropped on
    /// overflow. `serde(default)` so existing state files load without the
    /// field present.
    #[serde(default)]
    pub call_log: Vec<CallLogEntry>,
    /// Set once after the "consultation satisfied — retry skipped" log line
    /// has been emitted, so the completion pipeline doesn't re-log each tick.
    #[serde(skip)]
    pub consultation_skip_logged: bool,
    /// Set once the adapt follow-up overlay has been shown or checked for this
    /// session, so the completion pipeline doesn't re-parse `last_message`
    /// and re-surface the overlay after the user dismisses it.
    #[serde(skip)]
    pub adapt_follow_up_considered: bool,
    /// Provenance — `DirectUser` unless explicitly set by an orchestrator
    /// constructor. Used by `SessionPool::try_promote` to gate HTTP-template
    /// injection (issue #707).
    #[serde(default)]
    pub origin: SessionOrigin,
    /// Canonical command identifier (e.g., `"implement"`, `"pushup"`,
    /// `"plan-feature"`, `"simplify"`) when the session was spawned in the
    /// context of one. `None` for ad-hoc prompts. Used by
    /// `SessionPool::try_promote` to look up the rendered template body for
    /// HTTP-generic providers. See issue #707.
    #[serde(default)]
    pub active_command: Option<String>,
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
    pub fn new(
        prompt: String,
        model: String,
        mode: String,
        issue_number: Option<u64>,
        role_override: Option<super::role::Role>,
    ) -> Self {
        let intent = super::intent::classify_intent(&prompt);
        let role = role_override.unwrap_or_else(|| super::role::derive_role(&prompt));
        Self {
            id: Uuid::new_v4(),
            status: SessionStatus::Queued,
            prompt,
            issue_number,
            issue_numbers: Vec::new(),
            model,
            mode,
            session_mode: SessionMode::default(),
            agent_session_id: None,
            settled_from: None,
            turn_state: super::interaction::TurnState::Idle,
            turns: Vec::new(),
            produce_pr: false,
            pr_linked: None,
            agent_id: None,
            mode_config: None,
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
            hollow_dismissed: false,
            transition_flash_remaining: 0,
            is_thinking: false,
            thinking_started_at: None,
            tq_handoff_original_tokens: None,
            tq_handoff_compressed_tokens: None,
            worktree_path: None,
            transition_history: Vec::new(),
            intent,
            role,
            origin: SessionOrigin::default(),
            active_command: None,
            consultation_skip_logged: false,
            adapt_follow_up_considered: false,
            call_log: Vec::new(),
        }
    }

    /// Maximum number of [`CallLogEntry`] entries kept on a session.
    /// Mirrors the activity-log cap but ×5 because one prompt can produce
    /// many tool calls and tool results (#868).
    pub const CALL_LOG_CAP: usize = 500;

    /// Bind the provider-side conversation id (#947). The id is later
    /// reused verbatim as a `--resume <id>` argv operand, so it must pass
    /// the [`super::parser::is_valid_session_id`] allowlist (#751 security
    /// review). First bound id wins — later binds are ignored so a
    /// follow-up turn can never silently rebind the conversation.
    /// Returns `true` when the id was stored.
    pub fn bind_agent_session_id(&mut self, id: &str) -> bool {
        if self.agent_session_id.is_some() || !super::parser::is_valid_session_id(id) {
            return false;
        }
        self.agent_session_id = Some(id.to_string());
        true
    }

    /// Append a stream event to the persisted call log. Drops
    /// [`StreamEvent::Unknown`] (parse failure noise) and drains the oldest
    /// entries once [`Self::CALL_LOG_CAP`] is exceeded.
    pub fn append_call_log(&mut self, event: &StreamEvent) {
        let Some(kind) = CallLogKind::from_event(event) else {
            return;
        };
        self.call_log.push(CallLogEntry {
            timestamp: Utc::now(),
            kind,
            payload_json: render_event_payload(event),
        });
        if self.call_log.len() > Self::CALL_LOG_CAP {
            let excess = self.call_log.len() - Self::CALL_LOG_CAP;
            self.call_log.drain(..excess);
        }
    }

    /// Validated state transition. Records the transition in history.
    pub fn transition_to(
        &mut self,
        target: SessionStatus,
        reason: super::transition::TransitionReason,
    ) -> Result<(), super::transition::IllegalTransition> {
        // Kept-alive interception (#948): an Interactive-mode session that
        // would settle on a one-shot outcome stays alive instead. This is
        // the single choke point — every present and future terminal-status
        // site is covered structurally (spec 2026-06-04 §8). `Killed` is
        // NOT intercepted: explicit quit/kill really ends the session.
        let target = if self.session_mode == SessionMode::Interactive
            && matches!(
                target,
                SessionStatus::Completed
                    | SessionStatus::FailedGates
                    | SessionStatus::NeedsPr
                    | SessionStatus::Errored
            ) {
            self.settled_from = Some(target);
            SessionStatus::Interactive
        } else {
            target
        };
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

    pub fn with_mode_config(mut self, mode_config: Option<SessionModeConfig>) -> Self {
        self.mode_config = mode_config;
        self
    }

    pub fn with_agent_id(mut self, agent_id: Option<String>) -> Self {
        self.agent_id = agent_id;
        self
    }

    #[allow(dead_code)] // Reason: set by orchestrator constructors once L1/L2 flow through pool
    pub fn with_origin(mut self, origin: SessionOrigin) -> Self {
        self.origin = origin;
        self
    }

    #[allow(dead_code)] // Reason: set by command-invocation surfaces (#707 follow-up)
    pub fn with_active_command(mut self, command: Option<String>) -> Self {
        self.active_command = command;
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

/// Provider-neutral events emitted by agent output parsers.
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
        /// Subagent or skill name, if this is a known dispatcher tool
        /// (`Agent`/`Task` carry `input.subagent_type`; `Skill` carries
        /// `input.skill`). `None` for plain tool calls and for unidentified
        /// dispatches. See issue #542.
        subagent_name: Option<String>,
    },
    /// Tool result received
    ToolResult { tool: String, is_error: bool },
    /// Cost update from a non-terminal usage frame.
    ///
    /// Emitted by parsers when a provider reports cost on a frame distinct from
    /// the terminal `Completed` event (e.g. mid-turn usage snapshots). The handler
    /// in `ManagedSession::handle_event` replaces `Session.cost_usd` with the
    /// reported value. Parsers MUST drop frames where `cost_usd` is `NaN` or
    /// negative (see `parse_result` in `src/session/parser.rs`).
    ///
    /// Currently no shipped provider emits this variant; it is reserved for
    /// future split-frame providers and exercised end-to-end by tests in
    /// `src/integration_tests/stream_parsing.rs`.
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
    /// Provider-emitted operational anomaly worth surfacing in the activity
    /// log and structured log. Distinct from [`StreamEvent::Error`] (terminal)
    /// and [`StreamEvent::Unknown`] (parse failure). `code` is a stable
    /// kebab/snake_case tag callers branch on (e.g. `"quota_forced"`,
    /// `"token_count_clamped"`); `message` is human-readable for the TUI and
    /// log file.
    Warning { code: String, message: String },
    /// Output of a hook subprocess (`.maestro/hooks/*.sh`, plugin hooks).
    /// Surfaced in the per-agent call log so hook activity is visible
    /// alongside tool calls and assistant messages (#887). `stdout`/`stderr`
    /// are untrusted subprocess output — capped at [`PAYLOAD_TEXT_CAP`] before
    /// persistence.
    HookResponse {
        hook_name: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
}

/// Persisted classification of a [`StreamEvent`] kept in [`Session::call_log`]
/// for the per-agent call-log viewer (#868). [`StreamEvent::Unknown`] is
/// deliberately omitted — parse failures are not user-facing events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallLogKind {
    AssistantMessage,
    ToolUse,
    ToolResult,
    CostUpdate,
    Completed,
    Error,
    ContextUpdate,
    TokenUpdate,
    Thinking,
    Warning,
    HookResponse,
}

impl CallLogKind {
    /// Map a [`StreamEvent`] to its log kind. Returns `None` for
    /// `StreamEvent::Unknown` so parse-failure noise stays out of the log.
    pub fn from_event(event: &StreamEvent) -> Option<Self> {
        match event {
            StreamEvent::AssistantMessage { .. } => Some(Self::AssistantMessage),
            StreamEvent::ToolUse { .. } => Some(Self::ToolUse),
            StreamEvent::ToolResult { .. } => Some(Self::ToolResult),
            StreamEvent::CostUpdate { .. } => Some(Self::CostUpdate),
            StreamEvent::Completed { .. } => Some(Self::Completed),
            StreamEvent::Error { .. } => Some(Self::Error),
            StreamEvent::ContextUpdate { .. } => Some(Self::ContextUpdate),
            StreamEvent::TokenUpdate { .. } => Some(Self::TokenUpdate),
            StreamEvent::Thinking { .. } => Some(Self::Thinking),
            StreamEvent::Warning { .. } => Some(Self::Warning),
            StreamEvent::HookResponse { .. } => Some(Self::HookResponse),
            StreamEvent::Unknown { .. } => None,
        }
    }

    /// Short stable label rendered in the call-log row.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AssistantMessage => "AssistantMsg",
            Self::ToolUse => "ToolUse",
            Self::ToolResult => "ToolResult",
            Self::CostUpdate => "CostUpdate",
            Self::Completed => "Completed",
            Self::Error => "Error",
            Self::ContextUpdate => "Context",
            Self::TokenUpdate => "TokenUpdate",
            Self::Thinking => "Thinking",
            Self::Warning => "Warning",
            Self::HookResponse => "HookResponse",
        }
    }
}

/// One entry in [`Session::call_log`]. `payload_json` is the JSON-pretty
/// rendering captured at insertion time so the call-log viewer renders the
/// same payload it parsed, even after the session ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallLogEntry {
    pub timestamp: DateTime<Utc>,
    pub kind: CallLogKind,
    pub payload_json: String,
}

/// Per-text-field cap inside [`render_event_payload`]. Mirrors the
/// `last_message` 10 KB cap (`src/session/manager.rs`); prevents a runaway
/// provider from bloating `Session.call_log` and the on-disk state file.
const PAYLOAD_TEXT_CAP: usize = 10_000;

/// Truncate `s` to at most [`PAYLOAD_TEXT_CAP`] bytes at a UTF-8 boundary,
/// appending a `…[truncated]` marker if cut.
fn cap_text(s: &str) -> String {
    if s.len() <= PAYLOAD_TEXT_CAP {
        return s.to_string();
    }
    let boundary = crate::util::formatting::truncate_at_char_boundary(s, PAYLOAD_TEXT_CAP);
    format!("{}…[truncated]", &s[..boundary])
}

/// Render a [`StreamEvent`] payload to JSON-pretty form for the call-log
/// viewer's expanded panel. Each variant is rendered with stable keys so
/// snapshot tests remain deterministic. Large text fields are capped at
/// [`PAYLOAD_TEXT_CAP`] to bound on-disk growth.
pub fn render_event_payload(event: &StreamEvent) -> String {
    let value = match event {
        StreamEvent::AssistantMessage { text } => serde_json::json!({
            "type": "AssistantMessage",
            "text": cap_text(text),
        }),
        StreamEvent::ToolUse {
            tool,
            file_path,
            command_preview,
            subagent_name,
        } => serde_json::json!({
            "type": "ToolUse",
            "tool": tool,
            "file_path": file_path,
            "command_preview": command_preview,
            "subagent_name": subagent_name,
        }),
        StreamEvent::ToolResult { tool, is_error } => serde_json::json!({
            "type": "ToolResult",
            "tool": tool,
            "is_error": is_error,
        }),
        StreamEvent::CostUpdate { cost_usd } => serde_json::json!({
            "type": "CostUpdate",
            "cost_usd": cost_usd,
        }),
        StreamEvent::Completed { cost_usd } => serde_json::json!({
            "type": "Completed",
            "cost_usd": cost_usd,
        }),
        StreamEvent::Error { message } => serde_json::json!({
            "type": "Error",
            "message": cap_text(message),
        }),
        StreamEvent::ContextUpdate { context_pct } => serde_json::json!({
            "type": "ContextUpdate",
            "context_pct": context_pct,
        }),
        StreamEvent::TokenUpdate { usage } => serde_json::json!({
            "type": "TokenUpdate",
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens(),
        }),
        StreamEvent::Thinking { text } => serde_json::json!({
            "type": "Thinking",
            "text": cap_text(text),
        }),
        StreamEvent::Warning { code, message } => serde_json::json!({
            "type": "Warning",
            "code": code,
            "message": cap_text(message),
        }),
        StreamEvent::HookResponse {
            hook_name,
            exit_code,
            stdout,
            stderr,
        } => serde_json::json!({
            "type": "HookResponse",
            "hook_name": hook_name,
            "exit_code": exit_code,
            "stdout": cap_text(stdout),
            "stderr": cap_text(stderr),
        }),
        StreamEvent::Unknown { raw } => serde_json::json!({
            "type": "Unknown",
            "raw": cap_text(raw),
        }),
    };
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "<render error>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_new_initializes_fork_fields_to_defaults() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
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

    // --- Issue #734: SessionMode (one-shot vs interactive spawn) ---

    #[test]
    fn session_mode_default_is_one_shot() {
        assert_eq!(SessionMode::default(), SessionMode::OneShot);
    }

    #[test]
    fn session_mode_one_shot_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionMode::OneShot).unwrap();
        assert_eq!(json, r#""one_shot""#);
    }

    #[test]
    fn session_mode_interactive_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionMode::Interactive).unwrap();
        assert_eq!(json, r#""interactive""#);
    }

    #[test]
    fn session_mode_round_trips_via_serde() {
        for mode in [SessionMode::OneShot, SessionMode::Interactive] {
            let json = serde_json::to_string(&mode).unwrap();
            let rt: SessionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, mode);
        }
    }

    #[test]
    fn session_mode_from_false_is_one_shot() {
        assert_eq!(SessionMode::from(false), SessionMode::OneShot);
    }

    #[test]
    fn session_mode_from_true_is_interactive() {
        assert_eq!(SessionMode::from(true), SessionMode::Interactive);
    }

    // --- Issue #947: Session.session_mode field + agent_session_id bind ---

    fn make_947_session() -> Session {
        Session::new(
            "prompt".to_string(),
            "claude-sonnet-4-6".to_string(),
            "orchestrator".to_string(),
            Some(947),
            None,
        )
    }

    #[test]
    fn session_mode_field_defaults_to_one_shot() {
        assert_eq!(make_947_session().session_mode, SessionMode::OneShot);
    }

    #[test]
    fn session_mode_field_deserializes_missing_as_one_shot() {
        // Pre-#947 state files carry no `session_mode` key.
        let mut json = serde_json::to_value(make_947_session()).unwrap();
        json.as_object_mut().unwrap().remove("session_mode");
        let s: Session = serde_json::from_value(json).unwrap();
        assert_eq!(s.session_mode, SessionMode::OneShot);
    }

    #[test]
    fn agent_session_id_defaults_to_none() {
        assert!(make_947_session().agent_session_id.is_none());
    }

    #[test]
    fn bind_agent_session_id_accepts_valid_id() {
        let mut s = make_947_session();
        assert!(s.bind_agent_session_id("abc123-DEF_456"));
        assert_eq!(s.agent_session_id.as_deref(), Some("abc123-DEF_456"));
    }

    #[test]
    fn bind_agent_session_id_rejects_shell_metacharacters() {
        let mut s = make_947_session();
        assert!(!s.bind_agent_session_id("$(rm -rf /)"));
        assert!(s.agent_session_id.is_none());
    }

    #[test]
    fn bind_agent_session_id_rejects_flag_shaped_id() {
        let mut s = make_947_session();
        assert!(!s.bind_agent_session_id("--resume-injection"));
        assert!(s.agent_session_id.is_none());
    }

    #[test]
    fn bind_agent_session_id_first_bind_wins() {
        let mut s = make_947_session();
        assert!(s.bind_agent_session_id("first-id"));
        assert!(!s.bind_agent_session_id("second-id"));
        assert_eq!(s.agent_session_id.as_deref(), Some("first-id"));
    }

    // --- Issue #948: SessionStatus::Interactive + kept-alive settle ---

    fn make_interactive_session() -> Session {
        let mut s = make_947_session();
        s.session_mode = SessionMode::Interactive;
        s
    }

    fn drive_to_running(s: &mut Session) {
        use crate::session::transition::TransitionReason;
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
    }

    #[test]
    fn interactive_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::Interactive).unwrap();
        assert_eq!(json, r#""interactive""#);
    }

    #[test]
    fn interactive_status_has_label_and_symbol() {
        let status = SessionStatus::Interactive;
        assert!(!status.symbol().is_empty());
        assert_eq!(status.label(), "INTERACTIVE");
    }

    #[test]
    fn interactive_status_is_not_terminal() {
        assert!(!SessionStatus::Interactive.is_terminal());
        assert!(SessionStatus::Interactive.can_transition_to(SessionStatus::Killed));
        assert!(SessionStatus::Interactive.can_transition_to(SessionStatus::Interactive));
    }

    #[test]
    fn interactive_session_settles_to_interactive_on_completed() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Interactive);
        assert_eq!(s.settled_from, Some(SessionStatus::Completed));
    }

    #[test]
    fn interactive_session_failure_stays_alive() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::Errored, TransitionReason::StreamError)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Interactive);
        assert_eq!(s.settled_from, Some(SessionStatus::Errored));
    }

    #[test]
    fn interactive_session_gates_failure_stays_alive() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::GatesRunning, TransitionReason::GatesStarted)
            .unwrap();
        s.transition_to(SessionStatus::FailedGates, TransitionReason::GatesFailed)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Interactive);
        assert_eq!(s.settled_from, Some(SessionStatus::FailedGates));
    }

    #[test]
    fn interactive_session_needs_pr_stays_alive() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::NeedsPr, TransitionReason::PrNeeded)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Interactive);
        assert_eq!(s.settled_from, Some(SessionStatus::NeedsPr));
    }

    #[test]
    fn interactive_resettle_updates_settled_from() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::Errored, TransitionReason::StreamError)
            .unwrap();
        assert_eq!(s.settled_from, Some(SessionStatus::Errored));
        // Follow-up turn settles again — the banner shows the LATEST outcome.
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Interactive);
        assert_eq!(s.settled_from, Some(SessionStatus::Completed));
    }

    #[test]
    fn interactive_kill_passes_through_as_terminal() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        s.transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Killed);
        assert!(s.status.is_terminal());
    }

    #[test]
    fn one_shot_session_never_enters_the_interactive_tail() {
        use crate::session::transition::TransitionReason;
        let mut s = make_947_session(); // OneShot
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        assert_eq!(s.status, SessionStatus::Completed);
        assert_eq!(s.settled_from, None);
    }

    #[test]
    fn interception_records_interactive_in_transition_history() {
        use crate::session::transition::TransitionReason;
        let mut s = make_interactive_session();
        drive_to_running(&mut s);
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        let last = s.transition_history.last().unwrap();
        assert_eq!(last.to, SessionStatus::Interactive);
        assert_eq!(last.reason, TransitionReason::StreamCompleted);
    }

    #[test]
    fn turn_state_defaults_to_idle_and_is_not_persisted() {
        let s = make_interactive_session();
        assert_eq!(s.turn_state, crate::session::interaction::TurnState::Idle);
        let json = serde_json::to_value(&s).unwrap();
        assert!(
            json.get("turn_state").is_none(),
            "turn_state is in-memory turn activity, never persisted"
        );
    }

    #[test]
    fn turns_and_produce_pr_default_and_survive_missing_keys() {
        let mut json = serde_json::to_value(make_947_session()).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("turns");
        obj.remove("produce_pr");
        obj.remove("settled_from");
        let s: Session = serde_json::from_value(json).unwrap();
        assert!(s.turns.is_empty());
        assert!(!s.produce_pr);
        assert_eq!(s.settled_from, None);
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
            None,
        );
        assert!(s.ci_fix_context.is_none());
    }

    // --- image_paths field tests (issue #42) ---

    #[test]
    fn session_new_initializes_image_paths_as_empty() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(s.image_paths.is_empty());
    }

    #[test]
    fn session_with_image_paths_builder() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None)
            .with_image_paths(vec![
                std::path::PathBuf::from("/tmp/a.png"),
                std::path::PathBuf::from("/tmp/b.jpg"),
            ]);
        assert_eq!(s.image_paths.len(), 2);
    }

    #[test]
    fn session_with_image_paths_round_trips_via_serde() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None)
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","image_paths":[]"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.image_paths.is_empty());
    }

    // --- Issue #134: Thinking state fields ---

    #[test]
    fn session_is_thinking_defaults_to_false() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(!s.is_thinking);
        assert!(s.thinking_started_at.is_none());
    }

    #[test]
    fn session_thinking_fields_skipped_in_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
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
            subagent_name: None,
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
            subagent_name: None,
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

    // --- Issue #542: subagent_name on StreamEvent::ToolUse ---

    #[test]
    fn stream_event_tool_use_subagent_name_holds_value() {
        let e = StreamEvent::ToolUse {
            tool: "Agent".to_string(),
            file_path: None,
            command_preview: None,
            subagent_name: Some("subagent-architect".to_string()),
        };
        match e {
            StreamEvent::ToolUse { subagent_name, .. } => {
                assert_eq!(subagent_name, Some("subagent-architect".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn stream_event_tool_use_subagent_name_defaults_none_for_plain_tool() {
        let e = StreamEvent::ToolUse {
            tool: "Read".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            command_preview: None,
            subagent_name: None,
        };
        match e {
            StreamEvent::ToolUse { subagent_name, .. } => {
                assert_eq!(subagent_name, None);
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
            None,
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(s.gate_results.is_empty());
    }

    #[test]
    fn session_gate_results_round_trips_via_serde() {
        let mut s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
            None,
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(s.conflict_fix_context.is_none());
    }

    #[test]
    fn session_with_conflict_fix_context_round_trips_via_serde() {
        let mut s = Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
            None,
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","conflict_fix_context":null"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.conflict_fix_context.is_none());
    }

    // --- Issue #558: SessionStatus::FailedGates ---

    #[test]
    fn failed_gates_status_is_terminal() {
        assert!(SessionStatus::FailedGates.is_terminal());
    }

    #[test]
    fn failed_gates_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::FailedGates).unwrap();
        assert_eq!(json, r#""failed_gates""#);
    }

    #[test]
    fn failed_gates_status_deserializes_from_snake_case() {
        let status: SessionStatus = serde_json::from_str(r#""failed_gates""#).unwrap();
        assert_eq!(status, SessionStatus::FailedGates);
    }

    #[test]
    fn valid_transitions_gates_running_includes_failed_gates() {
        let transitions = SessionStatus::GatesRunning.valid_transitions();
        assert!(
            transitions.contains(&SessionStatus::FailedGates),
            "GatesRunning must allow transition to FailedGates"
        );
    }

    #[test]
    fn failed_gates_has_label_and_symbol() {
        let status = SessionStatus::FailedGates;
        assert!(!status.symbol().is_empty());
        assert_eq!(status.label(), "FAILED_GATES");
    }

    #[test]
    fn session_with_failed_gates_round_trips_via_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.status = SessionStatus::FailedGates;
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.status, SessionStatus::FailedGates);
    }

    // --- Issue #560: Session.worktree_path field for FailedGates recovery ---

    #[test]
    fn session_worktree_path_defaults_to_none() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(s.worktree_path.is_none());
    }

    #[test]
    fn session_worktree_path_round_trips_via_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.worktree_path = Some(std::path::PathBuf::from(".maestro/worktrees/issue-560"));
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(
            rt.worktree_path,
            Some(std::path::PathBuf::from(".maestro/worktrees/issue-560"))
        );
    }

    #[test]
    fn session_worktree_path_deserializes_with_default_when_field_absent() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","worktree_path":null"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.worktree_path.is_none());
    }

    // --- Issue #169: Hollow completion detection ---

    fn make_session_for_hollow() -> Session {
        Session::new(
            "test prompt".into(),
            "claude-sonnet-4-6".into(),
            "orchestrator".into(),
            None,
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
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
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert_eq!(s.transition_flash_remaining, 0);
    }

    #[test]
    fn transition_to_sets_flash_counter() {
        use crate::session::transition::TransitionReason;
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        assert_eq!(s.transition_flash_remaining, 4);
    }

    #[test]
    fn transition_to_resets_flash_counter_on_each_transition() {
        use crate::session::transition::TransitionReason;
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        s.transition_flash_remaining = 1; // simulate partial decay
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        assert_eq!(s.transition_flash_remaining, 4);
    }

    #[test]
    fn failed_transition_does_not_set_flash_counter() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
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
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        let Some(last) = s.activity_log.last() else {
            panic!("activity log should have entry");
        };
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

    // --- Issue #273: SessionIntent wiring on Session ---

    #[test]
    fn session_new_classifies_work_prompt_as_work() {
        let s = Session::new(
            "fix bug in login".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        assert_eq!(s.intent, crate::session::intent::SessionIntent::Work);
    }

    #[test]
    fn session_new_classifies_consultation_prompt_as_consultation() {
        let s = Session::new(
            "how are you?".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        assert_eq!(
            s.intent,
            crate::session::intent::SessionIntent::Consultation
        );
    }

    #[test]
    fn session_intent_round_trips_via_serde() {
        let s = Session::new(
            "explain the auth flow".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(
            rt.intent,
            crate::session::intent::SessionIntent::Consultation
        );
    }

    // --- Issue #346: tq_handoff_* serde backward compat ---

    #[test]
    fn session_tq_handoff_fields_default_to_none() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(s.tq_handoff_original_tokens.is_none());
        assert!(s.tq_handoff_compressed_tokens.is_none());
    }

    #[test]
    fn session_tq_handoff_fields_round_trip_via_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.tq_handoff_original_tokens = Some(10_000);
        s.tq_handoff_compressed_tokens = Some(2_500);
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.tq_handoff_original_tokens, Some(10_000));
        assert_eq!(rt.tq_handoff_compressed_tokens, Some(2_500));
    }

    #[test]
    fn session_tq_handoff_fields_deserialize_when_absent_in_json() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json
            .replace(r#","tq_handoff_original_tokens":null"#, "")
            .replace(r#","tq_handoff_compressed_tokens":null"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.tq_handoff_original_tokens.is_none());
        assert!(rt.tq_handoff_compressed_tokens.is_none());
    }

    #[test]
    fn session_intent_defaults_to_work_when_absent_in_json() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","intent":"work""#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert_eq!(rt.intent, crate::session::intent::SessionIntent::Work);
    }

    // --- Issue #538: Role wiring on Session ---

    #[test]
    fn session_new_classifies_orchestrator_prompt_as_orchestrator() {
        use crate::session::role::Role;
        let s = Session::new(
            "coordinate the sprint and dispatch tasks".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        assert_eq!(s.role, Role::Orchestrator);
    }

    #[test]
    fn session_new_classifies_implementer_prompt_as_implementer_default() {
        use crate::session::role::Role;
        let s = Session::new(
            "fix the bug in session/parser.rs".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        assert_eq!(s.role, Role::Implementer);
    }

    #[test]
    fn session_new_with_role_override_uses_override_not_derive() {
        use crate::session::role::Role;
        // Prompt would derive Orchestrator, but the explicit override forces Reviewer.
        let s = Session::new(
            "coordinate the release milestone".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            Some(Role::Reviewer),
        );
        assert_eq!(s.role, Role::Reviewer);
    }

    #[test]
    fn role_field_round_trips_via_serde() {
        use crate::session::role::Role;
        let s = Session::new(
            "coordinate the sprint".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        assert_eq!(s.role, Role::Orchestrator);
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.role, Role::Orchestrator);
    }

    #[test]
    fn role_defaults_to_implementer_when_absent_in_json() {
        use crate::session::role::Role;
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","role":"implementer""#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert_eq!(rt.role, Role::Implementer);
    }

    #[test]
    fn transition_to_does_not_mutate_role() {
        use crate::session::role::Role;
        use crate::session::transition::TransitionReason;
        let mut s = Session::new(
            "coordinate the milestone".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        assert_eq!(s.role, Role::Orchestrator);
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .unwrap();
        assert_eq!(s.role, Role::Orchestrator);
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .unwrap();
        assert_eq!(s.role, Role::Orchestrator);
        s.transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted)
            .unwrap();
        assert_eq!(s.role, Role::Orchestrator);
    }

    #[test]
    fn flash_counter_skipped_in_serde() {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        s.transition_flash_remaining = 4;
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("transition_flash_remaining"));
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.transition_flash_remaining, 0);
    }

    // --- Issue #707: SessionOrigin + active_command ---

    #[test]
    fn session_origin_default_is_direct_user() {
        assert_eq!(SessionOrigin::default(), SessionOrigin::DirectUser);
    }

    #[test]
    fn session_origin_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionOrigin::OrchestratorL1).unwrap();
        assert_eq!(json, r#""orchestrator_l1""#);
        let json = serde_json::to_string(&SessionOrigin::OrchestratorL2).unwrap();
        assert_eq!(json, r#""orchestrator_l2""#);
        let json = serde_json::to_string(&SessionOrigin::DirectUser).unwrap();
        assert_eq!(json, r#""direct_user""#);
    }

    #[test]
    fn session_origin_deserializes_from_snake_case() {
        let result: SessionOrigin = serde_json::from_str(r#""orchestrator_l2""#).unwrap();
        assert_eq!(result, SessionOrigin::OrchestratorL2);
    }

    #[test]
    fn session_origin_deserializes_with_default_when_field_absent() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","origin":"direct_user""#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert_eq!(rt.origin, SessionOrigin::DirectUser);
    }

    #[test]
    fn session_new_defaults_origin_to_direct_user() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert_eq!(s.origin, SessionOrigin::DirectUser);
    }

    #[test]
    fn session_new_defaults_active_command_to_none() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        assert!(s.active_command.is_none());
    }

    #[test]
    fn session_with_origin_builder_sets_origin() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None)
            .with_origin(SessionOrigin::OrchestratorL1);
        assert_eq!(s.origin, SessionOrigin::OrchestratorL1);
    }

    #[test]
    fn session_with_active_command_builder_sets_command() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None)
            .with_active_command(Some("implement".to_string()));
        assert_eq!(s.active_command.as_deref(), Some("implement"));
    }

    #[test]
    fn session_active_command_round_trips_via_serde() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None)
            .with_active_command(Some("implement".to_string()));
        let json = serde_json::to_string(&s).unwrap();
        let rt: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.active_command.as_deref(), Some("implement"));
    }

    #[test]
    fn session_active_command_deserializes_with_default_when_field_absent() {
        let s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        let json = serde_json::to_string(&s).unwrap();
        let stripped = json.replace(r#","active_command":null"#, "");
        let rt: Session = serde_json::from_str(&stripped).unwrap();
        assert!(rt.active_command.is_none());
    }

    // ---- #846 token-count sanitization ----

    #[test]
    fn token_count_cap_value_is_100_million() {
        assert_eq!(TOKEN_COUNT_CAP, 100_000_000_u64);
    }

    #[test]
    fn sanitize_token_count_passes_through_below_cap() {
        assert_eq!(sanitize_token_count(0), 0);
        assert_eq!(sanitize_token_count(1), 1);
        assert_eq!(
            sanitize_token_count(TOKEN_COUNT_CAP - 1),
            TOKEN_COUNT_CAP - 1
        );
    }

    #[test]
    fn sanitize_token_count_keeps_cap_value() {
        assert_eq!(sanitize_token_count(TOKEN_COUNT_CAP), TOKEN_COUNT_CAP);
    }

    #[test]
    fn sanitize_token_count_clamps_above_cap() {
        assert_eq!(sanitize_token_count(TOKEN_COUNT_CAP + 1), TOKEN_COUNT_CAP);
        assert_eq!(sanitize_token_count(u64::MAX), TOKEN_COUNT_CAP);
    }

    #[test]
    fn cost_per_kilo_token_finite_after_cap() {
        let capped = sanitize_token_count(u64::MAX);
        let usage = TokenUsage {
            input_tokens: capped,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };
        let cost = usage.cost_per_kilo_token(1.0);
        assert!(
            cost.is_finite(),
            "cost must be finite after cap, got {cost}"
        );
        assert!(cost >= 0.0);
    }

    // ---- #845 / #846 StreamEvent::Warning variant ----

    #[test]
    fn stream_event_warning_holds_code_and_message() {
        let e = StreamEvent::Warning {
            code: "quota_forced".to_string(),
            message: "MiniMax spawn forced over quota".to_string(),
        };
        match e {
            StreamEvent::Warning { code, message } => {
                assert_eq!(code, "quota_forced");
                assert!(message.contains("MiniMax"));
            }
            other => panic!("expected Warning, got {other:?}"),
        }
    }

    #[test]
    fn stream_event_warning_token_cap_code() {
        let e = StreamEvent::Warning {
            code: "token_count_clamped".to_string(),
            message: "claude: input_tokens=999 capped to 100000000".to_string(),
        };
        if let StreamEvent::Warning { code, .. } = e {
            assert_eq!(code, "token_count_clamped");
        } else {
            panic!("expected Warning");
        }
    }

    // =========================================================================
    // Issue #868 — CallLogEntry / Session::append_call_log
    // =========================================================================

    fn make_assistant_event() -> StreamEvent {
        StreamEvent::AssistantMessage {
            text: "Analyzing codebase".into(),
        }
    }

    fn make_tool_use_event() -> StreamEvent {
        StreamEvent::ToolUse {
            tool: "Read".into(),
            file_path: Some("src/lib.rs".into()),
            command_preview: None,
            subagent_name: None,
        }
    }

    fn make_unknown_event() -> StreamEvent {
        StreamEvent::Unknown {
            raw: "not json".into(),
        }
    }

    fn fresh_session() -> Session {
        Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None)
    }

    #[test]
    fn append_call_log_adds_entry_for_known_event() {
        let mut s = fresh_session();
        s.append_call_log(&make_assistant_event());
        assert_eq!(s.call_log.len(), 1);
    }

    #[test]
    fn append_call_log_unknown_event_is_dropped() {
        let mut s = fresh_session();
        s.append_call_log(&make_unknown_event());
        assert!(s.call_log.is_empty());
    }

    #[test]
    fn append_call_log_entry_has_correct_kind() {
        let mut s = fresh_session();
        s.append_call_log(&make_tool_use_event());
        assert_eq!(s.call_log[0].kind, CallLogKind::ToolUse);
    }

    #[test]
    fn append_call_log_entry_payload_json_is_non_empty() {
        let mut s = fresh_session();
        s.append_call_log(&make_assistant_event());
        assert!(!s.call_log[0].payload_json.is_empty());
    }

    #[test]
    fn append_call_log_respects_cap_drains_oldest() {
        let mut s = fresh_session();
        let total = Session::CALL_LOG_CAP + 10;
        for _ in 0..total {
            s.append_call_log(&make_assistant_event());
        }
        assert_eq!(s.call_log.len(), Session::CALL_LOG_CAP);
    }

    #[test]
    fn append_call_log_cap_preserves_most_recent_entries() {
        let mut s = fresh_session();
        for _ in 0..Session::CALL_LOG_CAP {
            s.append_call_log(&make_tool_use_event());
        }
        s.append_call_log(&make_assistant_event());
        assert_eq!(
            s.call_log.last().unwrap().kind,
            CallLogKind::AssistantMessage
        );
    }

    #[test]
    fn call_log_defaults_to_empty_on_new_session() {
        let s = fresh_session();
        assert!(s.call_log.is_empty());
    }

    #[test]
    fn call_log_field_survives_serde_round_trip() {
        let mut s = fresh_session();
        s.append_call_log(&make_assistant_event());
        let json = serde_json::to_string(&s).unwrap();
        let s2: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.call_log.len(), 1);
        assert_eq!(s2.call_log[0].kind, CallLogKind::AssistantMessage);
    }

    #[test]
    fn call_log_missing_from_json_deserializes_as_empty() {
        let json = r#"{
            "id":"00000000-0000-0000-0000-000000000000",
            "status":"queued",
            "prompt":"p","issue_number":null,
            "model":"opus","mode":"orchestrator",
            "started_at":null,"finished_at":null,
            "cost_usd":0.0,"context_pct":0.0,
            "current_activity":"","last_message":"",
            "activity_log":[],"files_touched":[],
            "pid":null
        }"#;
        let s: Session = serde_json::from_str(json).unwrap();
        assert!(s.call_log.is_empty());
    }

    #[test]
    fn call_log_kind_from_event_covers_all_renderable_variants() {
        let cases: &[(StreamEvent, CallLogKind)] = &[
            (
                StreamEvent::AssistantMessage { text: "hi".into() },
                CallLogKind::AssistantMessage,
            ),
            (
                StreamEvent::ToolUse {
                    tool: "Read".into(),
                    file_path: None,
                    command_preview: None,
                    subagent_name: None,
                },
                CallLogKind::ToolUse,
            ),
            (
                StreamEvent::ToolResult {
                    tool: "Read".into(),
                    is_error: false,
                },
                CallLogKind::ToolResult,
            ),
            (
                StreamEvent::CostUpdate { cost_usd: 0.01 },
                CallLogKind::CostUpdate,
            ),
            (
                StreamEvent::Completed { cost_usd: 0.5 },
                CallLogKind::Completed,
            ),
            (
                StreamEvent::Error {
                    message: "oops".into(),
                },
                CallLogKind::Error,
            ),
            (
                StreamEvent::ContextUpdate { context_pct: 0.42 },
                CallLogKind::ContextUpdate,
            ),
            (
                StreamEvent::TokenUpdate {
                    usage: TokenUsage::default(),
                },
                CallLogKind::TokenUpdate,
            ),
            (
                StreamEvent::Thinking { text: "...".into() },
                CallLogKind::Thinking,
            ),
            (
                StreamEvent::Warning {
                    code: "quota_forced".into(),
                    message: "rate limited".into(),
                },
                CallLogKind::Warning,
            ),
            (
                StreamEvent::HookResponse {
                    hook_name: "pre-commit".into(),
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                CallLogKind::HookResponse,
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(
                CallLogKind::from_event(event),
                Some(*expected),
                "wrong kind for {event:?}"
            );
        }
    }

    #[test]
    fn call_log_kind_from_event_unknown_returns_none() {
        let event = StreamEvent::Unknown {
            raw: "garbage".into(),
        };
        assert_eq!(CallLogKind::from_event(&event), None);
    }

    #[test]
    fn render_event_payload_caps_large_assistant_text() {
        let big = "x".repeat(20_000);
        let event = StreamEvent::AssistantMessage { text: big.clone() };
        let rendered = render_event_payload(&event);
        assert!(rendered.len() < 20_000);
        assert!(rendered.contains("…[truncated]"));
    }

    #[test]
    fn render_event_payload_small_text_not_truncated() {
        let event = StreamEvent::AssistantMessage {
            text: "short".into(),
        };
        let rendered = render_event_payload(&event);
        assert!(!rendered.contains("[truncated]"));
        assert!(rendered.contains("short"));
    }

    #[test]
    fn call_log_kind_label_non_empty_for_every_variant() {
        let kinds = [
            CallLogKind::AssistantMessage,
            CallLogKind::ToolUse,
            CallLogKind::ToolResult,
            CallLogKind::CostUpdate,
            CallLogKind::Completed,
            CallLogKind::Error,
            CallLogKind::ContextUpdate,
            CallLogKind::TokenUpdate,
            CallLogKind::Thinking,
            CallLogKind::Warning,
            CallLogKind::HookResponse,
        ];
        for kind in &kinds {
            assert!(!kind.label().is_empty(), "empty label for {kind:?}");
        }
    }

    // =========================================================================
    // Issue #887 — StreamEvent::HookResponse + CallLogKind::HookResponse
    // =========================================================================

    fn make_hook_response_event() -> StreamEvent {
        StreamEvent::HookResponse {
            hook_name: "pre-commit".into(),
            exit_code: 1,
            stdout: "stdout output".into(),
            stderr: "stderr output".into(),
        }
    }

    #[test]
    fn hook_response_kind_from_event_returns_some() {
        assert_eq!(
            CallLogKind::from_event(&make_hook_response_event()),
            Some(CallLogKind::HookResponse)
        );
    }

    #[test]
    fn hook_response_kind_label_is_hook_response() {
        assert_eq!(CallLogKind::HookResponse.label(), "HookResponse");
    }

    #[test]
    fn hook_response_kind_serializes_as_snake_case() {
        let json = serde_json::to_string(&CallLogKind::HookResponse).unwrap();
        assert_eq!(json, r#""hook_response""#);
    }

    #[test]
    fn hook_response_kind_deserializes_from_snake_case() {
        let kind: CallLogKind = serde_json::from_str(r#""hook_response""#).unwrap();
        assert_eq!(kind, CallLogKind::HookResponse);
    }

    #[test]
    fn render_event_payload_hook_response_includes_required_fields() {
        let event = StreamEvent::HookResponse {
            hook_name: "post-push".into(),
            exit_code: 2,
            stdout: "ok".into(),
            stderr: "warn".into(),
        };
        let rendered = render_event_payload(&event);
        assert!(rendered.contains("hook_name"), "missing hook_name key");
        assert!(rendered.contains("post-push"), "missing hook_name value");
        assert!(rendered.contains("exit_code"), "missing exit_code key");
        assert!(rendered.contains('2'), "missing exit_code value");
        assert!(rendered.contains("stdout"), "missing stdout key");
        assert!(rendered.contains("stderr"), "missing stderr key");
    }

    #[test]
    fn render_event_payload_hook_response_caps_stdout_at_10kb() {
        let event = StreamEvent::HookResponse {
            hook_name: "pre-commit".into(),
            exit_code: 0,
            stdout: "x".repeat(20_000),
            stderr: String::new(),
        };
        let rendered = render_event_payload(&event);
        assert!(rendered.len() < 20_000, "stdout should be capped");
        assert!(
            rendered.contains("…[truncated]"),
            "expected truncation marker"
        );
    }

    #[test]
    fn render_event_payload_hook_response_caps_stderr_at_10kb() {
        let event = StreamEvent::HookResponse {
            hook_name: "pre-commit".into(),
            exit_code: 1,
            stdout: String::new(),
            stderr: "e".repeat(20_000),
        };
        let rendered = render_event_payload(&event);
        assert!(rendered.len() < 20_000, "stderr should be capped");
        assert!(
            rendered.contains("…[truncated]"),
            "expected truncation marker"
        );
    }

    #[test]
    fn append_call_log_hook_response_entry_has_correct_kind() {
        let mut s = fresh_session();
        s.append_call_log(&make_hook_response_event());
        assert_eq!(s.call_log[0].kind, CallLogKind::HookResponse);
    }
}
