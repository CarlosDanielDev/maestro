//! Discovery of Claude Code session transcripts machine-wide (#1052).
//!
//! Walks `home/.claude/projects/<munged-cwd>/<session-id>.jsonl` and reads
//! each transcript's identity from the JSONL lines themselves — `sessionId`
//! and `cwd` live on the transcript lines, not in the munged directory name,
//! which is lossy (`/` and `.` both munge to `-`, so it cannot round-trip).
//! The module only reads; it never modifies what it scans.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

/// First-user-message titles longer than this are cut and closed with `…`.
const MAX_TITLE_CHARS: usize = 60;

/// One adoptable external Claude Code session (#1051 resumes it later).
#[derive(Debug)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub cwd: PathBuf,
    /// `ai-title` if the transcript has one, else the first user message
    /// truncated, else empty.
    pub title: String,
    /// Transcript file mtime — the only liveness signal this slice emits.
    /// A file whose mtime cannot be read falls back to `UNIX_EPOCH`, which
    /// sorts it last: unreadable mtime must not pass as recent activity.
    pub last_active: SystemTime,
    /// Lines that could not be read (IO or UTF-8 errors). The scan never
    /// stops at them; this count is how the damage stays visible.
    pub skipped_lines: usize,
}

/// Enumerate every readable transcript under `home/.claude/projects/`, most
/// recently active first. Entries missing a line-level `sessionId` or `cwd`
/// are skipped: without them a session cannot be identified or resumed
/// (#1051 locates the transcript by cwd).
pub fn scan_transcripts(home: &Path) -> Vec<DiscoveredSession> {
    let projects = home.join(".claude").join("projects");
    let Ok(project_dirs) = fs::read_dir(&projects) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for project in project_dirs.flatten() {
        // Symlinks are never followed: a planted link inside the projects
        // root must not send the scan at arbitrary outside targets.
        if project.file_type().map_or(true, |t| t.is_symlink()) {
            continue;
        }
        let Ok(transcripts) = fs::read_dir(project.path()) else {
            continue;
        };
        for transcript in transcripts.flatten() {
            if transcript.file_type().map_or(true, |t| t.is_symlink()) {
                continue;
            }
            let path = transcript.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(session) = read_session(&path) {
                sessions.push(session);
            }
        }
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active));
    sessions
}

/// Read one transcript JSONL into a [`DiscoveredSession`], or `None` when the
/// lines never state a `sessionId` or a `cwd`. The whole file is scanned:
/// the `ai-title` line may arrive after many turns, so an early exit would
/// silently lose it.
fn read_session(path: &Path) -> Option<DiscoveredSession> {
    let reader = BufReader::new(File::open(path).ok()?);
    let mut session_id = None;
    let mut cwd = None;
    let mut title = None;
    let mut fallback = None;
    let mut skipped_lines = 0usize;

    for item in reader.lines() {
        // A line that cannot be read (IO or UTF-8) never ends the scan;
        // the damage is counted on the session instead.
        let Ok(line) = item else {
            skipped_lines += 1;
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if session_id.is_none()
            && let Some(id) = v.get("sessionId").and_then(Value::as_str)
        {
            session_id = Some(id.to_string());
        }
        if cwd.is_none()
            && let Some(dir) = v.get("cwd").and_then(Value::as_str)
        {
            cwd = Some(PathBuf::from(dir));
        }
        if v.get("type").and_then(Value::as_str) == Some("ai-title")
            && let Some(t) = v.get("aiTitle").and_then(Value::as_str)
        {
            // Last one wins: it is the title Claude Code holds now.
            title = Some(t.to_string());
        }
        if fallback.is_none()
            && let Some(text) = plain_user_text(&v)
        {
            fallback = Some(text.to_string());
        }
    }

    Some(DiscoveredSession {
        session_id: session_id?,
        cwd: cwd?,
        title: title
            .or_else(|| fallback.as_deref().map(truncate_title))
            .unwrap_or_default(),
        last_active: fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH),
        skipped_lines,
    })
}

/// The prompt text of a plain user entry (string `message.content`) — that is
/// the user's own prompt; array content is a tool-result echo and never a
/// title source.
fn plain_user_text(v: &Value) -> Option<&str> {
    if v.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    v.get("message")?
        .get("content")?
        .as_str()
        .filter(|s| !s.trim().is_empty())
}

fn truncate_title(text: &str) -> String {
    let trimmed = text.trim();
    let long = trimmed.chars().count() > MAX_TITLE_CHARS;
    let head: String = trimmed.chars().take(MAX_TITLE_CHARS).collect();
    let mut out = head.trim_end().to_string();
    if long {
        out.push('…');
    }
    out
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
