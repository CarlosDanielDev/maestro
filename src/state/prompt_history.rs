use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Outcome of a prompt session for history tagging.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptOutcome {
    Completed,
    Hollow,
    Errored,
    Unknown,
}

/// A single prompt history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptHistoryEntry {
    pub prompt: String,
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<uuid::Uuid>,
    pub outcome: PromptOutcome,
}

/// Persistent store for prompt history, backed by a JSON file.
pub struct PromptHistoryStore {
    path: PathBuf,
    entries: Vec<PromptHistoryEntry>,
    max_entries: usize,
}

impl PromptHistoryStore {
    pub fn new(path: PathBuf, max_entries: usize) -> Self {
        Self {
            path,
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from("maestro-prompt-history.json")
    }

    pub fn set_max_entries(&mut self, n: usize) {
        self.max_entries = n;
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .with_context(|| format!("reading prompt history from {}", self.path.display()))?;
        self.entries = serde_json::from_str(&content)
            .with_context(|| format!("parsing prompt history from {}", self.path.display()))?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let content =
            serde_json::to_string_pretty(&self.entries).context("serializing prompt history")?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &content)
            .with_context(|| format!("writing prompt history to {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("renaming {} to {}", tmp.display(), self.path.display()))?;
        Ok(())
    }

    pub fn push(&mut self, entry: PromptHistoryEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.drain(..self.entries.len() - self.max_entries);
        }
        let _ = self.save();
    }

    pub fn entries(&self) -> &[PromptHistoryEntry] {
        &self.entries
    }

    #[allow(dead_code)] // Reason: collection length — standard API surface
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn update_outcome(&mut self, session_id: uuid::Uuid, outcome: PromptOutcome) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .rev()
            .find(|e| e.session_id == Some(session_id))
        {
            entry.outcome = outcome;
            let _ = self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(prompt: &str) -> PromptHistoryEntry {
        PromptHistoryEntry {
            prompt: prompt.to_string(),
            timestamp: Utc::now(),
            session_id: None,
            outcome: PromptOutcome::Unknown,
        }
    }

    #[test]
    fn push_appends_entry() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut store = PromptHistoryStore::new(tmp.path().to_path_buf(), 100);
        store.push(make_entry("hello"));
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries()[0].prompt, "hello");
    }

    #[test]
    fn push_truncates_at_max_entries() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut store = PromptHistoryStore::new(tmp.path().to_path_buf(), 3);
        for i in 0..5 {
            store.push(make_entry(&format!("prompt-{}", i)));
        }
        assert_eq!(store.len(), 3);
        assert_eq!(store.entries()[0].prompt, "prompt-2");
        assert_eq!(store.entries()[2].prompt, "prompt-4");
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let mut store = PromptHistoryStore::new(path.clone(), 100);
        store.push(make_entry("first"));
        store.push(make_entry("second"));

        let mut store2 = PromptHistoryStore::new(path, 100);
        store2.load().expect("load should succeed");
        assert_eq!(store2.len(), 2);
        assert_eq!(store2.entries()[0].prompt, "first");
        assert_eq!(store2.entries()[1].prompt, "second");
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let mut store =
            PromptHistoryStore::new(PathBuf::from("/tmp/nonexistent-maestro-test.json"), 100);
        store.load().expect("load should succeed for missing file");
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn load_corrupt_file_returns_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not json").unwrap();
        let mut store = PromptHistoryStore::new(tmp.path().to_path_buf(), 100);
        assert!(store.load().is_err());
    }

    #[test]
    fn update_outcome_finds_by_session_id() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut store = PromptHistoryStore::new(tmp.path().to_path_buf(), 100);
        let id = uuid::Uuid::new_v4();
        store.push(PromptHistoryEntry {
            prompt: "test".into(),
            timestamp: Utc::now(),
            session_id: Some(id),
            outcome: PromptOutcome::Unknown,
        });
        store.update_outcome(id, PromptOutcome::Completed);
        assert_eq!(store.entries()[0].outcome, PromptOutcome::Completed);
    }

    #[test]
    fn update_outcome_missing_session_is_noop() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut store = PromptHistoryStore::new(tmp.path().to_path_buf(), 100);
        store.push(make_entry("test"));
        store.update_outcome(uuid::Uuid::new_v4(), PromptOutcome::Errored);
        assert_eq!(store.entries()[0].outcome, PromptOutcome::Unknown);
    }

    #[test]
    fn default_path_returns_expected() {
        let path = PromptHistoryStore::default_path();
        assert_eq!(path, PathBuf::from("maestro-prompt-history.json"));
    }
}
