use super::types::MaestroState;
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from("maestro-state.json")
    }

    /// Acquire a lock file (shared or exclusive).
    fn lock_file(&self, exclusive: bool) -> Result<File> {
        let lock_path = self.path.with_extension("json.lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("opening lock file {}", lock_path.display()))?;

        if exclusive {
            file.lock()
        } else {
            file.lock_shared()
        }
        .with_context(|| {
            format!(
                "Failed to acquire {} lock on {}",
                if exclusive { "exclusive" } else { "shared" },
                lock_path.display()
            )
        })?;

        Ok(file)
    }

    pub fn load(&self) -> Result<MaestroState> {
        let _lock = self.lock_file(false)?;
        match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content)
                .with_context(|| format!("parsing state from {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(MaestroState::default()),
            Err(e) => Err(e)
                .with_context(|| format!("reading state from {}", self.path.display())),
        }
    }

    pub fn save(&self, state: &MaestroState) -> Result<()> {
        let _lock = self.lock_file(true)?;
        let content = serde_json::to_string_pretty(state).context("serializing state")?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &content)
            .with_context(|| format!("writing state to {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("renaming {} to {}", tmp.display(), self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::types::MaestroState;
    use std::sync::Arc;

    fn make_store() -> (tempfile::TempDir, StateStore) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-state.json");
        (dir, StateStore::new(path))
    }

    #[test]
    fn load_returns_default_when_no_file() {
        let (_dir, store) = make_store();
        let state = store.load().unwrap();
        assert!(state.sessions.is_empty());
        assert_eq!(state.total_cost_usd, 0.0);
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_dir, store) = make_store();
        let mut state = MaestroState::default();
        state.total_cost_usd = 42.5;
        store.save(&state).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.total_cost_usd, 42.5);
    }

    #[test]
    fn concurrent_saves_do_not_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("concurrent-state.json");
        let store = Arc::new(StateStore::new(path));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    let mut state = MaestroState::default();
                    state.total_cost_usd = i as f64;
                    store.save(&state).unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // File must be valid JSON after all concurrent writes
        let loaded = store.load().unwrap();
        assert!(loaded.total_cost_usd >= 0.0);
    }
}
