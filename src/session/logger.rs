use anyhow::Result;
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

use super::types::StreamEvent;

/// Per-session file logger that writes timestamped log entries.
pub struct SessionLogger {
    log_dir: PathBuf,
}

impl SessionLogger {
    pub fn new(log_dir: PathBuf) -> Self {
        Self { log_dir }
    }

    /// Default log directory: `.maestro/logs/`.
    pub fn default_dir() -> PathBuf {
        PathBuf::from(".maestro").join("logs")
    }

    /// Ensure the log directory exists.
    pub fn ensure_dir(&self) -> Result<()> {
        if !self.log_dir.exists() {
            fs::create_dir_all(&self.log_dir)?;
        }
        Ok(())
    }

    /// Append a log entry for a session.
    pub fn log_event(&self, session_id: Uuid, event: &StreamEvent) -> Result<()> {
        self.ensure_dir()?;
        let path = self.log_path(session_id);
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
        let entry = match event {
            StreamEvent::AssistantMessage { text } => {
                format!("[{}] ASSISTANT: {}\n", timestamp, text)
            }
            StreamEvent::ToolUse {
                tool, file_path, ..
            } => {
                let path_info = file_path
                    .as_deref()
                    .map(|p| format!(" ({})", p))
                    .unwrap_or_default();
                format!("[{}] TOOL: {}{}\n", timestamp, tool, path_info)
            }
            StreamEvent::ToolResult { tool, is_error } => {
                let status = if *is_error { "ERROR" } else { "OK" };
                format!("[{}] RESULT: {} -> {}\n", timestamp, tool, status)
            }
            StreamEvent::Completed { cost_usd } => {
                format!("[{}] COMPLETED: ${:.2}\n", timestamp, cost_usd)
            }
            StreamEvent::Error { message } => {
                format!("[{}] ERROR: {}\n", timestamp, message)
            }
            StreamEvent::CostUpdate { cost_usd } => {
                format!("[{}] COST: ${:.2}\n", timestamp, cost_usd)
            }
            StreamEvent::ContextUpdate { context_pct } => {
                format!("[{}] CONTEXT: {:.0}%\n", timestamp, context_pct * 100.0)
            }
            StreamEvent::Thinking { text } => {
                format!("[{}] THINKING: {}\n", timestamp, text)
            }
            StreamEvent::TokenUpdate { usage } => {
                format!(
                    "[{}] TOKENS: in={} out={} cache_r={} cache_w={}\n",
                    timestamp,
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_read_tokens,
                    usage.cache_creation_tokens
                )
            }
            StreamEvent::Unknown { raw } => {
                format!("[{}] UNKNOWN: {}\n", timestamp, raw)
            }
        };

        file.write_all(entry.as_bytes())?;
        Ok(())
    }

    /// Get the log file path for a session.
    pub fn log_path(&self, session_id: Uuid) -> PathBuf {
        self.log_dir.join(format!("{}.log", session_id))
    }

    /// List all session log files with metadata.
    pub fn list_logs(&self) -> Result<Vec<LogSummary>> {
        if !self.log_dir.exists() {
            return Ok(Vec::new());
        }

        let mut summaries = Vec::new();
        for entry in fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "log") {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let metadata = entry.metadata()?;
                let size_bytes = metadata.len();
                summaries.push(LogSummary {
                    session_id: name,
                    path,
                    size_bytes,
                });
            }
        }

        summaries.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(summaries)
    }

    /// Read the full contents of a session log.
    pub fn read_log(&self, session_id: Uuid) -> Result<String> {
        let path = self.log_path(session_id);
        Ok(fs::read_to_string(&path)?)
    }

    /// Clean up logs older than the given retention period.
    pub fn cleanup_old_logs(&self, retention_days: u64) -> Result<usize> {
        if !self.log_dir.exists() {
            return Ok(0);
        }

        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(retention_days * 24 * 3600);

        let mut removed = 0;
        for entry in fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if let Ok(modified) = metadata.modified()
                && modified < cutoff
            {
                fs::remove_file(entry.path())?;
                removed += 1;
            }
        }

        Ok(removed)
    }
}

/// Summary info about a log file.
#[derive(Debug, Clone)]
pub struct LogSummary {
    pub session_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_logger() -> (SessionLogger, TempDir) {
        let dir = TempDir::new().unwrap();
        let logger = SessionLogger::new(dir.path().to_path_buf());
        (logger, dir)
    }

    #[test]
    fn log_event_creates_file() {
        let (logger, _dir) = make_logger();
        let id = Uuid::new_v4();
        let event = StreamEvent::AssistantMessage {
            text: "hello".into(),
        };
        logger.log_event(id, &event).unwrap();
        assert!(logger.log_path(id).exists());
    }

    #[test]
    fn log_event_appends_entries() {
        let (logger, _dir) = make_logger();
        let id = Uuid::new_v4();
        logger
            .log_event(
                id,
                &StreamEvent::AssistantMessage {
                    text: "first".into(),
                },
            )
            .unwrap();
        logger
            .log_event(
                id,
                &StreamEvent::AssistantMessage {
                    text: "second".into(),
                },
            )
            .unwrap();
        let content = logger.read_log(id).unwrap();
        assert!(content.contains("first"));
        assert!(content.contains("second"));
    }

    #[test]
    fn log_event_tool_use() {
        let (logger, _dir) = make_logger();
        let id = Uuid::new_v4();
        let event = StreamEvent::ToolUse {
            tool: "Write".into(),
            file_path: Some("src/main.rs".into()),
            command_preview: None,
            subagent_name: None,
        };
        logger.log_event(id, &event).unwrap();
        let content = logger.read_log(id).unwrap();
        assert!(content.contains("TOOL: Write (src/main.rs)"));
    }

    #[test]
    fn log_event_completed() {
        let (logger, _dir) = make_logger();
        let id = Uuid::new_v4();
        let event = StreamEvent::Completed { cost_usd: 1.23 };
        logger.log_event(id, &event).unwrap();
        let content = logger.read_log(id).unwrap();
        assert!(content.contains("COMPLETED: $1.23"));
    }

    #[test]
    fn log_event_error() {
        let (logger, _dir) = make_logger();
        let id = Uuid::new_v4();
        let event = StreamEvent::Error {
            message: "something broke".into(),
        };
        logger.log_event(id, &event).unwrap();
        let content = logger.read_log(id).unwrap();
        assert!(content.contains("ERROR: something broke"));
    }

    #[test]
    fn list_logs_returns_summaries() {
        let (logger, _dir) = make_logger();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        logger
            .log_event(id1, &StreamEvent::AssistantMessage { text: "a".into() })
            .unwrap();
        logger
            .log_event(id2, &StreamEvent::AssistantMessage { text: "b".into() })
            .unwrap();
        let logs = logger.list_logs().unwrap();
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn list_logs_empty_dir() {
        let (logger, _dir) = make_logger();
        let logs = logger.list_logs().unwrap();
        assert!(logs.is_empty());
    }

    #[test]
    fn read_log_nonexistent_returns_err() {
        let (logger, _dir) = make_logger();
        assert!(logger.read_log(Uuid::new_v4()).is_err());
    }
}
