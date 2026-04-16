use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Phase of work a session is currently in, inferred from stream events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    /// Reading files, searching code (Read, Grep, Glob tools).
    Analyzing,
    /// Writing or editing files (Write, Edit tools).
    Implementing,
    /// Running tests or test-related commands.
    Testing,
    /// Creating a PR or pushing code.
    CreatingPR,
}

impl SessionPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Analyzing => "ANALYZING",
            Self::Implementing => "IMPLEMENTING",
            Self::Testing => "TESTING",
            Self::CreatingPR => "CREATING PR",
        }
    }
}

/// Progress state for a single session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionProgress {
    pub phase: SessionPhase,
    pub tools_used_count: u32,
    pub files_at_checkpoint: Vec<String>,
}

impl SessionProgress {
    pub fn new() -> Self {
        Self {
            phase: SessionPhase::Analyzing,
            tools_used_count: 0,
            files_at_checkpoint: Vec::new(),
        }
    }

    /// Update the phase based on a tool being used.
    pub fn on_tool_use(&mut self, tool_name: &str, file_path: Option<&str>) {
        self.tools_used_count += 1;

        // Track files touched
        if let Some(path) = file_path
            && !self.files_at_checkpoint.contains(&path.to_string())
        {
            self.files_at_checkpoint.push(path.to_string());
        }

        // Infer phase from tool name
        match tool_name {
            "Read" | "Grep" | "Glob" | "Bash"
                if self.phase == SessionPhase::Analyzing =>
            {
                // Stay in Analyzing
            }
            "Write" | "Edit"
                if self.phase != SessionPhase::Testing
                    && self.phase != SessionPhase::CreatingPR =>
            {
                self.phase = SessionPhase::Implementing;
            }
            _ => {}
        }
    }

    /// Update phase based on assistant message content.
    pub fn on_message(&mut self, text: &str) {
        let lower = text.to_lowercase();
        if lower.contains("cargo test")
            || lower.contains("running tests")
            || lower.contains("test pass")
            || lower.contains("test fail")
        {
            self.phase = SessionPhase::Testing;
        }
        if lower.contains("creating pr")
            || lower.contains("pull request")
            || lower.contains("git push")
        {
            self.phase = SessionPhase::CreatingPR;
        }
    }

    /// Generate a progress summary for retry context.
    #[allow(dead_code)] // Reason: progress summary for retry context prompt
    pub fn summary(&self) -> String {
        format!(
            "Phase: {}, Tools used: {}, Files touched: {}",
            self.phase.label(),
            self.tools_used_count,
            self.files_at_checkpoint.len()
        )
    }
}

impl Default for SessionProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks progress for all active sessions.
pub struct ProgressTracker {
    progress: HashMap<Uuid, SessionProgress>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            progress: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, session_id: Uuid) -> &mut SessionProgress {
        self.progress.entry(session_id).or_default()
    }

    pub fn get(&self, session_id: &Uuid) -> Option<&SessionProgress> {
        self.progress.get(session_id)
    }

    pub fn remove(&mut self, session_id: &Uuid) {
        self.progress.remove(session_id);
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_progress_starts_in_analyzing() {
        let p = SessionProgress::new();
        assert_eq!(p.phase, SessionPhase::Analyzing);
        assert_eq!(p.tools_used_count, 0);
        assert!(p.files_at_checkpoint.is_empty());
    }

    #[test]
    fn on_tool_use_increments_count() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Read", None);
        assert_eq!(p.tools_used_count, 1);
        p.on_tool_use("Grep", None);
        assert_eq!(p.tools_used_count, 2);
    }

    #[test]
    fn on_tool_use_tracks_files() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Write", Some("src/main.rs"));
        assert_eq!(p.files_at_checkpoint, vec!["src/main.rs"]);
    }

    #[test]
    fn on_tool_use_deduplicates_files() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Write", Some("src/main.rs"));
        p.on_tool_use("Edit", Some("src/main.rs"));
        assert_eq!(p.files_at_checkpoint.len(), 1);
    }

    #[test]
    fn write_tool_transitions_to_implementing() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Write", Some("src/lib.rs"));
        assert_eq!(p.phase, SessionPhase::Implementing);
    }

    #[test]
    fn edit_tool_transitions_to_implementing() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Edit", Some("src/lib.rs"));
        assert_eq!(p.phase, SessionPhase::Implementing);
    }

    #[test]
    fn read_tool_stays_in_analyzing() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Read", Some("src/main.rs"));
        assert_eq!(p.phase, SessionPhase::Analyzing);
    }

    #[test]
    fn on_message_detects_testing_phase() {
        let mut p = SessionProgress::new();
        p.on_message("Running cargo test now...");
        assert_eq!(p.phase, SessionPhase::Testing);
    }

    #[test]
    fn on_message_detects_pr_creation() {
        let mut p = SessionProgress::new();
        p.on_message("Creating PR for the changes");
        assert_eq!(p.phase, SessionPhase::CreatingPR);
    }

    #[test]
    fn summary_includes_all_info() {
        let mut p = SessionProgress::new();
        p.on_tool_use("Write", Some("src/a.rs"));
        p.on_tool_use("Edit", Some("src/b.rs"));
        let s = p.summary();
        assert!(s.contains("IMPLEMENTING"));
        assert!(s.contains("Tools used: 2"));
        assert!(s.contains("Files touched: 2"));
    }

    #[test]
    fn tracker_get_or_create_initializes() {
        let mut tracker = ProgressTracker::new();
        let id = Uuid::new_v4();
        let p = tracker.get_or_create(id);
        assert_eq!(p.phase, SessionPhase::Analyzing);
    }

    #[test]
    fn tracker_get_returns_existing() {
        let mut tracker = ProgressTracker::new();
        let id = Uuid::new_v4();
        tracker.get_or_create(id).on_tool_use("Write", None);
        let p = tracker.get(&id).unwrap();
        assert_eq!(p.phase, SessionPhase::Implementing);
    }

    #[test]
    fn tracker_get_returns_none_for_unknown() {
        let tracker = ProgressTracker::new();
        assert!(tracker.get(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn tracker_remove_cleans_up() {
        let mut tracker = ProgressTracker::new();
        let id = Uuid::new_v4();
        tracker.get_or_create(id);
        tracker.remove(&id);
        assert!(tracker.get(&id).is_none());
    }
}
