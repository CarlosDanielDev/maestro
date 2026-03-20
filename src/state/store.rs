use super::types::MaestroState;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

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

    pub fn load(&self) -> Result<MaestroState> {
        if !self.path.exists() {
            return Ok(MaestroState::default());
        }
        let content = std::fs::read_to_string(&self.path)
            .with_context(|| format!("reading state from {}", self.path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("parsing state from {}", self.path.display()))
    }

    pub fn save(&self, state: &MaestroState) -> Result<()> {
        let content = serde_json::to_string_pretty(state).context("serializing state")?;
        // Write to temp file then rename for atomicity
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &content)
            .with_context(|| format!("writing state to {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("renaming {} to {}", tmp.display(), self.path.display()))?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
