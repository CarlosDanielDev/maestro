use super::types::{CURRENT_STATE_VERSION, IssueRunState, MaestroState, TeamRun};
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub struct StateStore {
    path: PathBuf,
    /// When `false` (default), each session's `call_log` is dropped before the
    /// state is written to disk — memory-only. Set from
    /// `[sessions] call_log_persist` (#888). Enforced in `save`.
    call_log_persist: bool,
}

impl StateStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            call_log_persist: false,
        }
    }

    /// Builder: enable on-disk `call_log` persistence (default off).
    pub fn with_call_log_persist(mut self, persist: bool) -> Self {
        self.call_log_persist = persist;
        self
    }

    /// Toggle on-disk `call_log` persistence after construction (the prod
    /// store is built before config is loaded; `App::configure` calls this).
    pub fn set_call_log_persist(&mut self, persist: bool) {
        self.call_log_persist = persist;
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
        // O_NOFOLLOW refuses a symlinked lock path so a planted
        // `<state>.json.lock` symlink cannot redirect lock traffic to an
        // attacker-chosen target (#1032, found by the 2026-09 audit FS-4).
        let mut opts = OpenOptions::new();
        opts.create(true).truncate(false).write(true);
        #[cfg(unix)]
        {
            opts.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = opts
            .open(&lock_path)
            .map_err(|e| {
                #[cfg(unix)]
                if e.raw_os_error() == Some(libc::ELOOP) {
                    return anyhow::anyhow!(
                        "refusing to open symlinked lock file {}",
                        lock_path.display()
                    );
                }
                anyhow::Error::new(e)
            })
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
        // Drop sensitive per-session call logs from the on-disk copy unless
        // explicitly opted in. Clone only when stripping so the common
        // (memory-only) path costs one clone of a history-capped session set,
        // and the caller's in-memory `state` is never mutated (#888).
        let content = if self.call_log_persist {
            serde_json::to_string_pretty(state).context("serializing state")?
        } else {
            let mut stripped = state.clone();
            for session in &mut stripped.sessions {
                session.call_log.clear();
            }
            serde_json::to_string_pretty(&stripped).context("serializing state")?
        };
        // NamedTempFile gives a random name with O_EXCL semantics, so a
        // planted `<state>.json.tmp` symlink cannot redirect the write
        // (#1032); `persist` renames it atomically over the real path.
        let parent = self.path.parent().unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("creating temp file next to {}", self.path.display()))?;
        tmp.write_all(content.as_bytes())
            .with_context(|| format!("writing state to {}", tmp.path().display()))?;
        tmp.flush()
            .with_context(|| format!("flushing state to {}", tmp.path().display()))?;
        tmp.persist(&self.path)
            .with_context(|| format!("persisting state to {}", self.path.display()))?;
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
