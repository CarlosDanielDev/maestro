use super::parser::parse_stream_line;
use super::types::{Session, SessionStatus, StreamEvent};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

/// Events sent from a session back to the coordinator/TUI.
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: uuid::Uuid,
    pub event: StreamEvent,
}

pub struct ManagedSession {
    pub session: Session,
    child: Option<Child>,
    /// Path to the git worktree for this session (Phase 1).
    pub worktree_path: Option<PathBuf>,
    /// System prompt appendix for file claims injection (Phase 1).
    pub system_prompt_appendix: Option<String>,
    /// Permission mode for Claude CLI (e.g., "bypassPermissions").
    pub permission_mode: Option<String>,
    /// Allowed tools whitelist.
    pub allowed_tools: Vec<String>,
}

impl ManagedSession {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            child: None,
            worktree_path: None,
            system_prompt_appendix: None,
            permission_mode: None,
            allowed_tools: Vec::new(),
        }
    }

    /// Create a managed session with worktree and file claims context.
    pub fn with_worktree(
        session: Session,
        worktree_path: Option<PathBuf>,
        system_prompt_appendix: Option<String>,
    ) -> Self {
        Self {
            session,
            child: None,
            worktree_path,
            system_prompt_appendix,
            permission_mode: None,
            allowed_tools: Vec::new(),
        }
    }

    /// Spawn the Claude CLI process and start streaming events.
    pub async fn spawn(&mut self, tx: mpsc::UnboundedSender<SessionEvent>) -> Result<()> {
        self.session.status = SessionStatus::Spawning;
        self.session.started_at = Some(Utc::now());

        let mut cmd = Command::new("claude");
        cmd.args(["--print", "--verbose", "--output-format", "stream-json"]);

        // Model selection
        cmd.args(["--model", &self.session.model]);

        // Permission mode (default: bypassPermissions for unattended sessions)
        if let Some(ref mode) = self.permission_mode
            && !mode.is_empty()
            && mode != "default"
        {
            cmd.args(["--permission-mode", mode]);
        }

        // Allowed tools whitelist
        if !self.allowed_tools.is_empty() {
            cmd.args(["--allowedTools", &self.allowed_tools.join(",")]);
        }

        // Inject file claims via --append-system-prompt
        if let Some(ref appendix) = self.system_prompt_appendix {
            cmd.args(["--append-system-prompt", appendix]);
        }

        // Set working directory to worktree if available
        if let Some(ref wt_path) = self.worktree_path {
            cmd.current_dir(wt_path);
        }

        // Prompt is a positional argument (must be last)
        cmd.arg(&self.session.prompt);

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn claude CLI")?;

        let pid = child.id().unwrap_or(0);
        self.session.pid = Some(pid);
        self.session.status = SessionStatus::Running;
        self.session
            .log_activity(format!("Session spawned (pid: {})", pid));

        let stdout = child.stdout.take().context("No stdout from claude CLI")?;
        let stderr = child.stderr.take();
        let session_id = self.session.id;

        // Stream reader task (stdout)
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut got_result = false;

            while let Ok(Some(line)) = lines.next_line().await {
                let event = parse_stream_line(&line);
                if matches!(event, StreamEvent::Completed { .. }) {
                    got_result = true;
                }
                let _ = tx.send(SessionEvent { session_id, event });
            }

            // Only send fallback completion if we didn't get a real result event
            if !got_result {
                let _ = tx.send(SessionEvent {
                    session_id,
                    event: StreamEvent::Completed { cost_usd: 0.0 },
                });
            }
        });

        // Stderr reader task — capture errors from Claude CLI
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut stderr_buf = String::new();

                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        if !stderr_buf.is_empty() {
                            stderr_buf.push('\n');
                        }
                        stderr_buf.push_str(&line);
                    }
                }

                if !stderr_buf.is_empty() {
                    let _ = tx2.send(SessionEvent {
                        session_id,
                        event: StreamEvent::Error {
                            message: stderr_buf,
                        },
                    });
                }
            });
        }

        self.child = Some(child);
        Ok(())
    }

    /// Send SIGSTOP to pause.
    #[cfg(unix)]
    pub fn pause(&self) -> Result<()> {
        if let Some(pid) = self.session.pid {
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
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
        }
        Ok(())
    }

    /// Kill the child process.
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            child.kill().await.context("Failed to kill session")?;
        }
        self.session.status = SessionStatus::Killed;
        self.session.finished_at = Some(Utc::now());
        self.session.log_activity("Session killed".into());
        Ok(())
    }

    /// Wait for the child to exit and return its status.
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        if let Some(ref mut child) = self.child {
            let status = child.wait().await?;
            Ok(status)
        } else {
            anyhow::bail!("No child process")
        }
    }

    /// Update session state from a stream event.
    pub fn handle_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::AssistantMessage { text } => {
                // Accumulate the full response for display in panel
                if !text.is_empty() {
                    if !self.session.last_message.is_empty() {
                        self.session.last_message.push('\n');
                    }
                    self.session.last_message.push_str(text);
                    // Cap at 10KB to prevent unbounded growth
                    if self.session.last_message.len() > 10_000 {
                        let start = self.session.last_message.len() - 8_000;
                        let boundary = truncate_at_char_boundary(&self.session.last_message, start);
                        self.session.last_message =
                            self.session.last_message[boundary..].to_string();
                    }
                }
                self.session.current_activity = "Thinking".into();
            }
            StreamEvent::ToolUse {
                tool, file_path, ..
            } => {
                self.session.current_activity = format!("Using {}", tool);
                self.session.log_activity(format!("Tool: {}", tool));

                // Track files touched
                if let Some(path) = file_path
                    && matches!(tool.as_str(), "Read" | "Edit" | "Write" | "Glob" | "Grep")
                    && !self.session.files_touched.contains(path)
                {
                    self.session.files_touched.push(path.clone());
                }
            }
            StreamEvent::ToolResult { tool, is_error } => {
                if *is_error {
                    self.session.log_activity(format!("Tool {} errored", tool));
                }
            }
            StreamEvent::CostUpdate { cost_usd } => {
                self.session.cost_usd = *cost_usd;
            }
            StreamEvent::Completed { cost_usd } => {
                if *cost_usd > 0.0 {
                    self.session.cost_usd = *cost_usd;
                }
                if !self.session.status.is_terminal() {
                    self.session.status = SessionStatus::Completed;
                    self.session.finished_at = Some(Utc::now());
                    self.session.current_activity = "Done".into();
                    self.session.log_activity("Session completed".into());
                }
            }
            StreamEvent::Error { message } => {
                self.session.status = SessionStatus::Errored;
                self.session.finished_at = Some(Utc::now());
                self.session.current_activity = "Error".into();
                self.session.log_activity(format!("Error: {}", message));
            }
            StreamEvent::Unknown { .. } => {}
        }
    }
}

use crate::util::truncate_at_char_boundary;
