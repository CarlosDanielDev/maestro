use super::types::{CURRENT_STATE_VERSION, IssueRunState, MaestroState, TeamRun};
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
        let mut state = match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content)
                .with_context(|| format!("parsing state from {}", self.path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => MaestroState::default(),
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("reading state from {}", self.path.display()));
            }
        };
        migrate(&mut state)?;
        reconcile_team_runs(&mut state.team_runs);
        Ok(state)
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

/// Bring an in-memory `MaestroState` up to `CURRENT_STATE_VERSION`.
///
/// Files written before the version stamp deserialize with `version: 0`
/// (via `default_state_version`); this is a no-op structural migration —
/// every later-added field already carries `#[serde(default)]`, so the
/// stamp bump is the only mutation required for `0 → 1`.
///
/// Returns an error when the state file is from a *newer* maestro version
/// than this binary supports (`state.version > CURRENT_STATE_VERSION`).
/// Silently re-saving in that case would discard unknown fields and
/// downgrade the file format — an OWASP A08 data-integrity risk
/// (#665 security review).
///
/// Idempotent: a state already at `CURRENT_STATE_VERSION` is unchanged.
pub fn migrate(state: &mut MaestroState) -> Result<()> {
    if state.version > CURRENT_STATE_VERSION {
        return Err(anyhow::anyhow!(
            "state file is from a newer maestro version (v{}); this build only knows up to v{} — upgrade maestro to load it",
            state.version,
            CURRENT_STATE_VERSION
        ));
    }
    if state.version < CURRENT_STATE_VERSION {
        state.version = CURRENT_STATE_VERSION;
    }
    Ok(())
}

pub fn reconcile_team_run(run: &mut TeamRun) {
    for state in run.state.values_mut() {
        if let IssueRunState::InFlight { .. } = state {
            *state = IssueRunState::Failed {
                reason: "process state lost across restart".into(),
                attempts: 0,
            };
        }
    }
}

fn reconcile_team_runs(runs: &mut [TeamRun]) {
    for run in runs {
        reconcile_team_run(run);
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
