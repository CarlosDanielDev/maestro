//! Persistence for the PRD model (#321).
//!
//! Stores at `<repo_root>/.maestro/prd.toml`. Atomic write-rename pattern
//! so a crashed write cannot leave a half-empty file.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #321. FilePrdStore is constructed by the
// PRD screen + CLI subcommand in Phase 2; tests exercise round-trip today.
#![allow(dead_code)]

use crate::prd::model::Prd;
use std::path::{Path, PathBuf};

/// Errors raised by the file-backed store.
#[derive(Debug)]
pub enum PrdStoreError {
    Io(std::io::Error),
    Toml(String),
}

impl std::fmt::Display for PrdStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "prd store i/o: {e}"),
            Self::Toml(msg) => write!(f, "prd store toml: {msg}"),
        }
    }
}

impl std::error::Error for PrdStoreError {}

impl From<std::io::Error> for PrdStoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Trait so the store can be faked in tests of layers above.
pub trait PrdStore: Send + Sync {
    fn prd_path(&self) -> PathBuf;
    fn load(&self) -> Result<Option<Prd>, PrdStoreError>;
    fn save(&self, prd: &Prd) -> Result<(), PrdStoreError>;
}

/// File-backed store that persists `prd.toml` under `<root>/.maestro/`.
pub struct FilePrdStore {
    root: PathBuf,
}

impl FilePrdStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn maestro_dir(&self) -> PathBuf {
        self.root.join(".maestro")
    }
}

impl PrdStore for FilePrdStore {
    fn prd_path(&self) -> PathBuf {
        self.maestro_dir().join("prd.toml")
    }

    fn load(&self) -> Result<Option<Prd>, PrdStoreError> {
        // Single syscall — no exists()+read race. Translate NotFound as
        // "no PRD yet" instead of an I/O error.
        let body = match std::fs::read_to_string(self.prd_path()) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let prd: Prd = toml::from_str(&body).map_err(|e| PrdStoreError::Toml(e.to_string()))?;
        Ok(Some(prd))
    }

    fn save(&self, prd: &Prd) -> Result<(), PrdStoreError> {
        use std::io::Write as _;

        let dir = self.maestro_dir();
        std::fs::create_dir_all(&dir)?;
        let body = toml::to_string_pretty(prd).map_err(|e| PrdStoreError::Toml(e.to_string()))?;
        let target = self.prd_path();
        let tmp = atomic_temp_path(&target);

        // O_EXCL + randomized suffix: refuses to follow a pre-existing
        // file/symlink in `.maestro/`, blocking the classic TOCTOU swap.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&tmp, &target)?;
        Ok(())
    }
}

fn atomic_temp_path(target: &Path) -> PathBuf {
    // Randomized suffix prevents an attacker on a shared host from
    // pre-creating the tmp path as a symlink. Combined with `create_new`
    // above, even a guessed suffix would error out instead of being
    // followed.
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let mut tmp = target.as_os_str().to_owned();
    tmp.push(".tmp.");
    tmp.push(&suffix);
    PathBuf::from(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prd::model::Goal;
    use tempfile::tempdir;

    fn sample_prd() -> Prd {
        let mut p = Prd::new();
        p.vision = "Ship".into();
        p.goals.push(Goal::new("Build the foundation"));
        p
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = tempdir().expect("tempdir");
        let store = FilePrdStore::new(dir.path());
        assert!(store.load().expect("load").is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().expect("tempdir");
        let store = FilePrdStore::new(dir.path());
        let prd = sample_prd();
        store.save(&prd).expect("save");
        let loaded = store.load().expect("load").expect("present");
        assert_eq!(loaded, prd);
    }

    #[test]
    fn save_creates_maestro_dir() {
        let dir = tempdir().expect("tempdir");
        let store = FilePrdStore::new(dir.path());
        store.save(&sample_prd()).expect("save");
        assert!(dir.path().join(".maestro").is_dir());
    }

    #[test]
    fn save_overwrites_existing_file() {
        let dir = tempdir().expect("tempdir");
        let store = FilePrdStore::new(dir.path());
        store.save(&sample_prd()).expect("save 1");
        let mut updated = sample_prd();
        updated.vision = "Updated".into();
        store.save(&updated).expect("save 2");
        let loaded = store.load().expect("load").expect("present");
        assert_eq!(loaded.vision, "Updated");
    }

    #[test]
    fn save_does_not_leave_tmp_files_behind() {
        let dir = tempdir().expect("tempdir");
        let store = FilePrdStore::new(dir.path());
        store.save(&sample_prd()).expect("save");
        // After save, only `prd.toml` should remain in `.maestro/`.
        let entries: Vec<_> = std::fs::read_dir(dir.path().join(".maestro"))
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["prd.toml"]);
    }

    #[test]
    fn atomic_temp_path_is_randomized_per_call() {
        let target = PathBuf::from("/tmp/x/.maestro/prd.toml");
        let a = atomic_temp_path(&target);
        let b = atomic_temp_path(&target);
        assert_ne!(a, b, "tmp path must be randomized to defeat symlink races");
    }

    #[test]
    fn load_invalid_toml_returns_toml_error() {
        let dir = tempdir().expect("tempdir");
        let store = FilePrdStore::new(dir.path());
        std::fs::create_dir_all(dir.path().join(".maestro")).expect("mkdir");
        std::fs::write(store.prd_path(), b"this is not valid toml = = =").expect("write");
        match store.load() {
            Err(PrdStoreError::Toml(_)) => (),
            other => panic!("expected toml error, got {other:?}"),
        }
    }

    #[test]
    fn prd_path_is_under_dot_maestro() {
        let store = FilePrdStore::new("/tmp/x");
        assert_eq!(store.prd_path(), PathBuf::from("/tmp/x/.maestro/prd.toml"));
    }
}
