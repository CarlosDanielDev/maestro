use super::types::{Session, SessionStatus, StreamEvent};
use crate::agent_provider::{
    AgentError, AgentProvider, AgentProviderEvent, AgentRequest, ClaudeProvider,
};
use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Sentinel `StreamEvent::Warning.code` used by the spawn handshake to
/// carry the "provider started" signal back to the main event loop without
/// blocking the spawn caller. The optional process id (subprocess
/// providers only) rides in `message` as a base-10 integer; HTTP providers
/// send an empty `message`.
///
/// Consumers MUST treat this code as a lifecycle marker — NOT as a warning
/// to surface in the activity log. The handler in
/// `App::handle_session_event` and `ManagedSession::handle_event`
/// special-cases it before the generic Warning path runs.
pub const SESSION_SPAWNED_CODE: &str = "session_spawned";

/// Events sent from a session back to the coordinator/TUI.
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: uuid::Uuid,
    pub event: StreamEvent,
}

pub struct ManagedSession {
    pub session: Session,
    provider: Arc<dyn AgentProvider>,
    cancel_token: Option<CancellationToken>,
    /// Path to the git worktree for this session (Phase 1).
    pub worktree_path: Option<PathBuf>,
    /// Branch name for this session's worktree (e.g., "maestro/issue-42").
    pub branch_name: Option<String>,
    /// System prompt appendix for file claims injection (Phase 1).
    pub system_prompt_appendix: Option<String>,
    /// Permission mode for Claude CLI (e.g., "bypassPermissions").
    pub permission_mode: Option<String>,
    /// Allowed tools whitelist.
    pub allowed_tools: Vec<String>,
    last_tool_start: Option<std::time::Instant>,
    thinking_start: Option<std::time::Instant>,
}

impl ManagedSession {
    #[allow(dead_code)] // Reason: constructor for managed session — used by session pool
    pub fn new(session: Session) -> Self {
        Self {
            session,
            provider: Arc::new(ClaudeProvider::default()),
            cancel_token: None,
            worktree_path: None,
            branch_name: None,
            system_prompt_appendix: None,
            permission_mode: None,
            allowed_tools: Vec::new(),
            last_tool_start: None,
            thinking_start: None,
        }
    }

    /// Create a managed session with worktree and file claims context.
    pub fn with_worktree(
        session: Session,
        worktree_path: Option<PathBuf>,
        branch_name: Option<String>,
        system_prompt_appendix: Option<String>,
    ) -> Self {
        Self {
            session,
            provider: Arc::new(ClaudeProvider::default()),
            cancel_token: None,
            worktree_path,
            branch_name,
            system_prompt_appendix,
            permission_mode: None,
            allowed_tools: Vec::new(),
            last_tool_start: None,
            thinking_start: None,
        }
    }

    pub fn set_provider(&mut self, provider: Arc<dyn AgentProvider>) {
        self.provider = provider;
    }

    #[cfg(test)]
    fn set_claude_binary_for_test(&mut self, binary: impl Into<String>) {
        self.provider = Arc::new(ClaudeProvider::new(binary));
    }

    fn build_request(&self) -> AgentRequest {
        let mut request =
            AgentRequest::stream_json(self.session.prompt.clone(), self.session.model.clone());
        request.cwd.clone_from(&self.worktree_path);
        request.images.clone_from(&self.session.image_paths);
        request.permission_mode.clone_from(&self.permission_mode);
        request.allowed_tools.clone_from(&self.allowed_tools);
        request
            .system_prompt_appendix
            .clone_from(&self.system_prompt_appendix);
        request
    }

    /// Start the configured agent provider and stream events back to Maestro.
    ///
    /// **Non-blocking.** The call transitions the session to
    /// [`SessionStatus::Spawning`] synchronously, kicks off two background
    /// tasks (one for the provider, one to forward events back), and
    /// returns immediately. The session remains in `Spawning` until the
    /// provider sends its first event:
    ///
    /// * `AgentProviderEvent::Started` → forwarded as
    ///   `StreamEvent::Warning { code: SESSION_SPAWNED_CODE, message: pid }`.
    ///   The main thread translates this into the
    ///   `Spawning → Running` transition and stamps `session.pid`.
    /// * `AgentProviderEvent::Stream(StreamEvent::Error { .. })` → forwarded
    ///   verbatim; `ManagedSession::handle_event` runs the
    ///   `Spawning → Errored` transition.
    ///
    /// This was made non-blocking in #803 because providers that perform a
    /// network handshake before `Started` (currently `minimax`, future
    /// HTTP-only providers) used to freeze the TUI's main event loop until
    /// the round-trip completed, then snap the screen to the session view
    /// with no visible "connecting…" feedback. Now the existing
    /// `Spawning` animation in `spinner.rs` is visible during the wait.
    pub async fn spawn(&mut self, tx: mpsc::UnboundedSender<SessionEvent>) -> Result<()> {
        use crate::session::transition::TransitionReason;
        let _ = self
            .session
            .transition_to(SessionStatus::Spawning, TransitionReason::Promoted);
        self.session.started_at = Some(Utc::now());

        let request = self.build_request();
        let provider = Arc::clone(&self.provider);
        let provider_id = provider.id().to_string();
        let cancel = CancellationToken::new();
        self.cancel_token = Some(cancel.clone());
        let (provider_tx, mut provider_rx) = mpsc::unbounded_channel();
        let session_id = self.session.id;

        tokio::spawn(async move {
            if let Err(err) = provider.run(request, provider_tx.clone(), cancel).await {
                if matches!(err, AgentError::Cancelled { .. }) {
                    return;
                }
                let _ = provider_tx.send(AgentProviderEvent::Stream(StreamEvent::Error {
                    message: err.to_string(),
                }));
            }
        });

        tokio::spawn(async move {
            let mut handshake_done = false;
            while let Some(event) = provider_rx.recv().await {
                match event {
                    AgentProviderEvent::Started(started) => {
                        if handshake_done {
                            // Provider should only emit Started once; ignore
                            // duplicates instead of double-transitioning.
                            continue;
                        }
                        handshake_done = true;
                        let pid_msg = started
                            .process_id
                            .map(|p| p.to_string())
                            .unwrap_or_default();
                        let _ = tx.send(SessionEvent {
                            session_id,
                            event: StreamEvent::Warning {
                                code: SESSION_SPAWNED_CODE.to_string(),
                                message: pid_msg,
                            },
                        });
                    }
                    AgentProviderEvent::Stream(stream_event) => {
                        handshake_done =
                            handshake_done || matches!(stream_event, StreamEvent::Error { .. });
                        let _ = tx.send(SessionEvent {
                            session_id,
                            event: stream_event,
                        });
                    }
                }
            }
            if !handshake_done {
                let _ = tx.send(SessionEvent {
                    session_id,
                    event: StreamEvent::Error {
                        message: format!("{provider_id} provider exited before startup"),
                    },
                });
            }
        });

        Ok(())
    }

    /// Send SIGSTOP to pause.
    #[cfg(unix)]
    pub fn pause(&self) -> Result<()> {
        if let Some(pid) = self.session.pid {
            // SAFETY: libc::kill is an FFI call; the pid comes from a session
            // we spawned (stored in self.session.pid). Passing SIGSTOP is
            // side-effect-only and cannot cause UB in this process. Return
            // value is intentionally ignored — a kill error (e.g. ESRCH for a
            // child that already exited) is handled by the caller via the
            // subsequent state transition, not by unwinding here.
            #[allow(unsafe_code)]
            unsafe {
                libc::kill(pid as i32, libc::SIGSTOP);
            }
        }
        Ok(())
    }

    /// Send SIGCONT to resume.
    #[cfg(unix)]
    pub fn resume(&self) -> Result<()> {
        if let Some(pid) = self.session.pid {
            // SAFETY: see `pause()` above — same rationale applies.
            #[allow(unsafe_code)]
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
        }
        Ok(())
    }

    /// Kill the child process.
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(cancel) = self.cancel_token.take() {
            cancel.cancel();
        }
        let _ = self.session.transition_to(
            SessionStatus::Killed,
            crate::session::transition::TransitionReason::UserKill,
        );
        self.session.log_activity("Session killed".into());
        Ok(())
    }

    /// Update session state from a stream event.
    pub fn handle_event(&mut self, event: &StreamEvent) {
        if !matches!(event, StreamEvent::Thinking { .. })
            && let Some(start) = self.thinking_start.take()
        {
            self.session.is_thinking = false;
            self.session.thinking_started_at = None;
            self.session
                .log_activity(format!("Thought for {}", format_elapsed(start.elapsed())));
        }

        match event {
            StreamEvent::AssistantMessage { text } => {
                if !text.is_empty() {
                    if text.len() > 40 {
                        let boundary = truncate_at_char_boundary(text, 40);
                        self.session.current_activity = format!("{}…", &text[..boundary]);
                    } else {
                        self.session.current_activity = text.clone();
                    }

                    if !self.session.last_message.is_empty() {
                        self.session.last_message.push('\n');
                    }
                    self.session.last_message.push_str(text);
                    if self.session.last_message.len() > 10_000 {
                        let start = self.session.last_message.len() - 8_000;
                        let boundary = truncate_at_char_boundary(&self.session.last_message, start);
                        self.session.last_message =
                            self.session.last_message[boundary..].to_string();
                    }
                }
            }
            StreamEvent::ToolUse {
                tool,
                file_path,
                command_preview,
                ..
            } => {
                self.last_tool_start = Some(std::time::Instant::now());

                let (activity, log_msg) = match (
                    tool.as_str(),
                    file_path.as_deref(),
                    command_preview.as_deref(),
                ) {
                    ("Bash", _, Some(cmd)) => (format!("$ {}", cmd), format!("Bash: $ {}", cmd)),
                    (t, Some(path), _) => {
                        let short = path.rsplit('/').next().unwrap_or(path);
                        (format!("{}: {}", t, short), format!("{}: {}", t, path))
                    }
                    (t, None, _) => (format!("Using {}", t), format!("Tool: {}", t)),
                };
                self.session.current_activity = activity;
                self.session.log_activity(log_msg);

                if let Some(path) = file_path
                    && matches!(tool.as_str(), "Read" | "Edit" | "Write" | "Glob" | "Grep")
                    && !self.session.files_touched.contains(path)
                {
                    self.session.files_touched_previous = self.session.files_touched.len();
                    self.session.files_touched.push(path.clone());
                }
            }
            StreamEvent::ToolResult { tool, is_error } => {
                let elapsed_str = self
                    .last_tool_start
                    .take()
                    .map(|start| format!(" ({})", format_elapsed(start.elapsed())))
                    .unwrap_or_default();

                if *is_error {
                    self.session
                        .log_activity(format!("Tool {} errored{}", tool, elapsed_str));
                } else {
                    self.session
                        .log_activity(format!("Tool {} done{}", tool, elapsed_str));
                }
            }
            StreamEvent::CostUpdate { cost_usd } => {
                self.session.cost_usd = *cost_usd;
            }
            StreamEvent::Completed { cost_usd } => {
                if *cost_usd > 0.0 {
                    self.session.cost_usd = *cost_usd;
                }
                if self
                    .session
                    .transition_to(
                        SessionStatus::Completed,
                        crate::session::transition::TransitionReason::StreamCompleted,
                    )
                    .is_ok()
                {
                    self.session.current_activity = "Done".into();
                    self.session.log_activity("Session completed".into());

                    if self.session.detect_hollow_completion() {
                        self.session.is_hollow_completion = true;
                        self.session.log_activity(
                            "Warning: hollow completion detected — session completed without performing any work".into(),
                        );
                    }
                }
            }
            StreamEvent::Error { message } => {
                let _ = self.session.transition_to(
                    SessionStatus::Errored,
                    crate::session::transition::TransitionReason::StreamError,
                );
                self.session.current_activity = "Error".into();
                self.session.log_activity(format!("Error: {}", message));
            }
            StreamEvent::Warning { code, message } if code == SESSION_SPAWNED_CODE => {
                // Non-blocking spawn handshake (see `spawn` docs): translate
                // the synthetic Spawned marker into the Spawning → Running
                // transition + pid stamp that the old blocking code did
                // inline on the spawn caller.
                if matches!(self.session.status, SessionStatus::Spawning) {
                    let pid = message.parse::<u32>().ok();
                    self.session.pid = pid;
                    let _ = self.session.transition_to(
                        SessionStatus::Running,
                        crate::session::transition::TransitionReason::Spawned,
                    );
                    let activity = match pid {
                        Some(p) => format!("Session spawned (pid: {p})"),
                        None => "Session spawned".into(),
                    };
                    self.session.log_activity(activity);
                }
            }
            StreamEvent::ContextUpdate { context_pct } => {
                self.session.context_pct = *context_pct;
                self.session
                    .log_activity(format!("Context: {:.0}%", context_pct * 100.0));
            }
            StreamEvent::TokenUpdate { usage } => {
                // Claude CLI emits cumulative totals, so we replace rather than accumulate
                self.session.token_usage = usage.clone();
            }
            StreamEvent::Thinking { .. } => {
                if self.thinking_start.is_none() {
                    let now = std::time::Instant::now();
                    self.thinking_start = Some(now);
                    self.session.is_thinking = true;
                    self.session.thinking_started_at = Some(now);
                    self.session.log_activity("Thinking...".into());
                }
                if self.session.current_activity != "Thinking..." {
                    self.session.current_activity = "Thinking...".into();
                }
            }
            StreamEvent::Unknown { .. } => {}
            StreamEvent::Warning { code, message } => {
                self.session
                    .log_activity(format!("WARNING [{code}]: {message}"));
            }
        }
    }
}

fn format_elapsed(d: std::time::Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

use crate::util::truncate_at_char_boundary;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::Session;
    use uuid::Uuid;

    fn make_managed(prompt: &str) -> ManagedSession {
        let session = Session {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            model: "claude-sonnet-4-5-20250514".to_string(),
            status: SessionStatus::Queued,
            issue_number: None,
            issue_numbers: vec![],
            mode: "print".to_string(),
            agent_id: None,
            mode_config: None,
            started_at: None,
            finished_at: None,
            cost_usd: 0.0,
            context_pct: 0.0,
            token_usage: crate::session::types::TokenUsage::default(),
            current_activity: String::new(),
            last_message: String::new(),
            activity_log: vec![],
            files_touched: vec![],
            files_touched_previous: 0,
            pid: None,
            issue_title: None,
            retry_count: 0,
            last_retry_at: None,
            parent_session_id: None,
            child_session_ids: vec![],
            fork_depth: 0,
            ci_fix_context: None,
            conflict_fix_context: None,
            image_paths: vec![],
            gate_results: vec![],
            is_hollow_completion: false,
            transition_flash_remaining: 0,
            is_thinking: false,
            thinking_started_at: None,
            tq_handoff_original_tokens: None,
            tq_handoff_compressed_tokens: None,
            worktree_path: None,
            transition_history: vec![],
            intent: crate::session::intent::SessionIntent::default(),
            role: crate::session::role::Role::default(),
            consultation_skip_logged: false,
            adapt_follow_up_considered: false,
            origin: crate::session::types::SessionOrigin::default(),
            active_command: None,
        };
        ManagedSession::new(session)
    }

    #[test]
    fn spawn_args_do_not_include_bare_flag() {
        let ms = make_managed("do something");
        let args = ClaudeProvider::default().build_stream_args(&ms.build_request());
        assert!(
            !args.iter().any(|a| a == "--bare"),
            "args must NOT contain --bare (breaks OAuth); got: {:?}",
            args
        );
    }

    #[test]
    fn spawn_args_include_required_base_flags() {
        let ms = make_managed("test prompt");
        let args = ClaudeProvider::default().build_stream_args(&ms.build_request());
        for flag in &["--print", "--verbose", "--output-format", "stream-json"] {
            assert!(
                args.iter().any(|a| a == flag),
                "args must contain {}; got: {:?}",
                flag,
                args
            );
        }
    }

    #[test]
    fn spawn_args_with_permission_mode_includes_permission_flag() {
        let mut ms = make_managed("test");
        ms.permission_mode = Some("bypassPermissions".to_string());
        let args = ClaudeProvider::default().build_stream_args(&ms.build_request());
        assert!(args.iter().any(|a| a == "--permission-mode"));
        assert!(args.iter().any(|a| a == "bypassPermissions"));
    }

    #[test]
    fn spawn_args_default_permission_mode_is_excluded() {
        let mut ms = make_managed("test");
        ms.permission_mode = Some("default".to_string());
        let args = ClaudeProvider::default().build_stream_args(&ms.build_request());
        assert!(
            !args.iter().any(|a| a == "--permission-mode"),
            "permission_mode=default must not emit --permission-mode flag"
        );
    }

    #[tokio::test]
    async fn spawn_invalid_binary_surfaces_error_via_channel() {
        // Post-#803 semantics: spawn() returns Ok immediately and the
        // session sits in Spawning until the background drain task
        // delivers either SESSION_SPAWNED_CODE or StreamEvent::Error.
        // For an invalid binary the provider fails before sending any
        // event, so the drain task synthesizes the "exited before
        // startup" Error.
        let mut ms = make_managed("test");
        ms.set_claude_binary_for_test("/definitely/not/a/claude/binary");
        let (tx, mut rx) = mpsc::unbounded_channel();

        ms.spawn(tx).await.expect("spawn returns immediately");
        assert_eq!(ms.session.status, SessionStatus::Spawning);

        let evt = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("error event arrives within timeout")
            .expect("channel still open");
        assert!(matches!(evt.event, StreamEvent::Error { .. }));

        // Feed the event through handle_event to drive the
        // Spawning → Errored transition the App would do.
        ms.handle_event(&evt.event);
        assert_eq!(ms.session.status, SessionStatus::Errored);
        assert!(
            ms.session
                .activity_log
                .iter()
                .any(|entry| entry.message.starts_with("Error:")),
            "spawn failure should be recorded in the session activity log"
        );
    }

    #[test]
    fn handle_event_session_spawned_warning_transitions_to_running() {
        // Sentinel `Warning { code = SESSION_SPAWNED_CODE }` is the
        // lifecycle marker emitted by the non-blocking spawn handshake.
        // It must transition Spawning → Running and stamp pid; for any
        // other status it must be a no-op so a stray late-arrival event
        // can't yank a Completed session back to Running.
        let mut ms = make_managed("test");
        ms.session.status = SessionStatus::Spawning;
        ms.handle_event(&StreamEvent::Warning {
            code: SESSION_SPAWNED_CODE.to_string(),
            message: "12345".to_string(),
        });
        assert_eq!(ms.session.status, SessionStatus::Running);
        assert_eq!(ms.session.pid, Some(12345));
        assert!(
            ms.session
                .activity_log
                .iter()
                .any(|entry| entry.message.contains("Session spawned (pid: 12345)"))
        );
    }

    #[test]
    fn handle_event_session_spawned_warning_noop_when_not_spawning() {
        let mut ms = make_managed("test");
        ms.session.status = SessionStatus::Completed;
        ms.handle_event(&StreamEvent::Warning {
            code: SESSION_SPAWNED_CODE.to_string(),
            message: "999".to_string(),
        });
        assert_eq!(ms.session.status, SessionStatus::Completed);
        assert_eq!(ms.session.pid, None);
    }

    #[test]
    fn handle_event_session_spawned_warning_handles_empty_pid_message() {
        // HTTP providers (minimax, ollama) have no pid; the drain task
        // emits an empty message in that case.
        let mut ms = make_managed("test");
        ms.session.status = SessionStatus::Spawning;
        ms.handle_event(&StreamEvent::Warning {
            code: SESSION_SPAWNED_CODE.to_string(),
            message: String::new(),
        });
        assert_eq!(ms.session.status, SessionStatus::Running);
        assert_eq!(ms.session.pid, None);
        assert!(
            ms.session
                .activity_log
                .iter()
                .any(|entry| entry.message == "Session spawned")
        );
    }

    // --- Issue #169: Hollow completion detection (handle_event) ---

    fn make_managed_with_start(prompt: &str, started_secs_ago: i64) -> ManagedSession {
        let mut ms = make_managed(prompt);
        ms.session.status = SessionStatus::Running;
        let now = chrono::Utc::now();
        ms.session.started_at = Some(now - chrono::Duration::seconds(started_secs_ago));
        ms
    }

    #[test]
    fn handle_event_completed_sets_hollow_flag_when_all_conditions_met() {
        let mut ms = make_managed_with_start("test", 10);
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });
        assert!(ms.session.is_hollow_completion);
        assert_eq!(ms.session.status, SessionStatus::Completed);
    }

    #[test]
    fn handle_event_completed_does_not_set_hollow_flag_when_cost_is_nonzero() {
        let mut ms = make_managed_with_start("test", 10);
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.03 });
        assert!(!ms.session.is_hollow_completion);
        assert!((ms.session.cost_usd - 0.03).abs() < f64::EPSILON);
    }

    #[test]
    fn handle_event_completed_does_not_set_hollow_flag_when_files_were_touched() {
        let mut ms = make_managed_with_start("test", 10);
        ms.session.files_touched.push("src/lib.rs".into());
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });
        assert!(!ms.session.is_hollow_completion);
    }

    #[test]
    fn handle_event_completed_does_not_set_hollow_flag_when_tool_calls_present() {
        let mut ms = make_managed_with_start("test", 10);
        ms.session.log_activity("Bash: $ cargo test".into());
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });
        assert!(!ms.session.is_hollow_completion);
    }

    #[test]
    fn handle_event_completed_logs_hollow_warning_to_activity_log() {
        let mut ms = make_managed_with_start("test", 10);
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });
        assert!(
            ms.session
                .activity_log
                .iter()
                .any(|e| e.message.to_lowercase().contains("hollow")),
            "expected a hollow warning entry in the activity log"
        );
    }

    #[test]
    fn handle_event_completed_does_not_log_hollow_warning_for_real_session() {
        let mut ms = make_managed_with_start("test", 60);
        ms.session.cost_usd = 0.05;
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.05 });
        assert!(
            !ms.session
                .activity_log
                .iter()
                .any(|e| e.message.to_lowercase().contains("hollow")),
            "must not log hollow warning for a real session"
        );
    }

    #[test]
    fn handle_event_completed_does_not_mutate_hollow_flag_for_already_terminal_session() {
        let mut ms = make_managed_with_start("test", 10);
        ms.session.status = SessionStatus::Killed;
        ms.session.is_hollow_completion = false;
        ms.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });
        assert_eq!(ms.session.status, SessionStatus::Killed);
        assert!(!ms.session.is_hollow_completion);
    }
}
