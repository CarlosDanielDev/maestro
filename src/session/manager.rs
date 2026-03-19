use super::parser::parse_stream_line;
use super::types::{Session, SessionStatus, StreamEvent};
use anyhow::{Context, Result};
use chrono::Utc;
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
}

impl ManagedSession {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            child: None,
        }
    }

    /// Spawn the Claude CLI process and start streaming events.
    pub async fn spawn(&mut self, tx: mpsc::UnboundedSender<SessionEvent>) -> Result<()> {
        self.session.status = SessionStatus::Spawning;
        self.session.started_at = Some(Utc::now());

        let mut cmd = Command::new("claude");
        cmd.args(["--print", "--output-format", "stream-json"]);

        // Model selection
        cmd.args(["--model", &self.session.model]);

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
        let session_id = self.session.id;

        // Stream reader task
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let event = parse_stream_line(&line);
                let _ = tx.send(SessionEvent {
                    session_id,
                    event,
                });
            }

            // Signal completion when stream ends
            let _ = tx.send(SessionEvent {
                session_id,
                event: StreamEvent::Completed { cost_usd: 0.0 },
            });
        });

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
                let truncated = if text.len() > 120 {
                    format!("{}…", &text[..120])
                } else {
                    text.clone()
                };
                self.session.last_message = truncated.clone();
                self.session.current_activity = "Thinking".into();
            }
            StreamEvent::ToolUse { tool, .. } => {
                self.session.current_activity = format!("Using {}", tool);
                self.session
                    .log_activity(format!("Tool: {}", tool));

                // Track files
                if matches!(tool.as_str(), "Read" | "Edit" | "Write" | "Glob" | "Grep") {
                    // File tracking will be enhanced in Phase 1
                }
            }
            StreamEvent::ToolResult { tool, is_error } => {
                if *is_error {
                    self.session
                        .log_activity(format!("Tool {} errored", tool));
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
                self.session
                    .log_activity(format!("Error: {}", message));
            }
            StreamEvent::Unknown { .. } => {}
        }
    }
}
