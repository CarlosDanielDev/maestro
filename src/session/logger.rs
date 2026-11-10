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
            StreamEvent::Warning { code, message } => {
                format!("[{}] WARNING [{}]: {}\n", timestamp, code, message)
            }
            StreamEvent::HookResponse {
                hook_name,
                exit_code,
                ..
            } => {
                // One-line summary in the flat log; full stdout/stderr lives
                // in the call-log viewer payload (#887).
                format!(
                    "[{}] HOOK: {} -> exit {}\n",
                    timestamp, hook_name, exit_code
                )
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
    ///
    /// #1031: the log root must be a real directory — never a symlink or a
    /// plain file (a cloned repo can commit `.maestro/logs` as either) — and
    /// only regular files this module writes (`<uuid>.log`) are ever removed.
    /// Per-entry failures are logged and skipped so one bad entry cannot abort
    /// the batch.
    pub fn cleanup_old_logs(&self, retention_days: u64) -> Result<usize> {
        let Ok(meta) = fs::symlink_metadata(&self.log_dir) else {
            // Missing root (or unreadable parent) — nothing to clean.
            return Ok(0);
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            tracing::warn!(
                "refusing to clean session logs: {} is a symlink or not a directory",
                self.log_dir.display()
            );
            return Ok(0);
        }

        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(retention_days * 24 * 3600);

        let mut removed = 0;
        for entry in fs::read_dir(&self.log_dir)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    tracing::warn!(
                        "skipping unreadable entry in {}: {}",
                        self.log_dir.display(),
                        e
                    );
                    continue;
                }
            };
            let path = entry.path();
            // `DirEntry::file_type` does not traverse symlinks, so a symlinked
            // entry fails `is_file()` and is left alone (#1031).
            let is_own_log = entry.file_type().is_ok_and(|t| t.is_file())
                && path.extension().is_some_and(|ext| ext == "log")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| Uuid::parse_str(stem).is_ok());
            if !is_own_log {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(e) => {
                    tracing::warn!("skipping {}: metadata unavailable: {}", path.display(), e);
                    continue;
                }
            };
            if let Ok(modified) = metadata.modified()
                && modified < cutoff
            {
                match fs::remove_file(&path) {
                    Ok(()) => removed += 1,
                    Err(e) => {
                        tracing::warn!(
                            "failed to remove old session log {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
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
#[path = "logger_tests.rs"]
mod tests;
