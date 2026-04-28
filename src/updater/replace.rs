use crate::updater::error::UpdateError;
use crate::updater::lock::UpdateLock;
use std::path::{Path, PathBuf};

/// Atomically replace a binary at a target path with new bytes.
///
/// Implementations must:
/// - acquire a single-writer lock at `<target>.update.lock` for the duration
/// - write the new bytes to a sibling temp path on the same filesystem
/// - back up the original (atomic rename of original to `<target>.bak`)
/// - atomically rename the temp into the target's place
/// - on any failure between backup and rename, restore from backup
/// - leave no partial files on disk in any error path
pub trait BinaryReplacer: Send + Sync {
    fn replace(&self, target: &Path, new_bytes: &[u8]) -> Result<ReplaceOutcome, UpdateError>;
}

/// Outcome of a successful replace: where the original was backed up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceOutcome {
    pub backup_path: PathBuf,
}

const BACKUP_SUFFIX: &str = ".bak";
const LOCK_SUFFIX: &str = ".update.lock";
const FALLBACK_NAME: &str = "maestro";

fn sibling_with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut p = target.to_path_buf();
    let name = p
        .file_name()
        .map(|n| format!("{}{}", n.to_string_lossy(), suffix))
        .unwrap_or_else(|| format!("{FALLBACK_NAME}{suffix}"));
    p.set_file_name(name);
    p
}

/// Production implementation backed by `tempfile::NamedTempFile` + `fs::rename`.
///
/// Strategy: write new bytes to a sibling NamedTempFile (same FS for atomic
/// rename), rename original to `<target>.bak`, promote the temp into place,
/// then chmod 0o755 on Unix. On promotion failure, restore from `.bak`. A
/// flock-based `UpdateLock` rejects concurrent attempts.
pub struct AtomicBinaryReplacer;

impl AtomicBinaryReplacer {
    pub fn new() -> Self {
        Self
    }

    fn write_temp(parent: &Path, new_bytes: &[u8]) -> Result<tempfile::NamedTempFile, UpdateError> {
        use std::io::Write;

        let tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => UpdateError::PermissionDenied {
                path: parent.to_path_buf(),
                source: e,
            },
            _ => UpdateError::Internal(format!("creating temp file in {}: {e}", parent.display())),
        })?;
        let mut file = tmp.as_file();
        file.write_all(new_bytes)
            .map_err(|e| UpdateError::Internal(format!("writing temp file: {e}")))?;
        file.sync_all()
            .map_err(|e| UpdateError::Internal(format!("fsyncing temp file: {e}")))?;
        // chmod is deferred until after `persist` so the world-exec bit is
        // never set while the file still bears its random `.tmp…` name.
        Ok(tmp)
    }

    fn map_rename_err(err: std::io::Error, path: &Path, ctx: &str) -> UpdateError {
        match err.kind() {
            std::io::ErrorKind::PermissionDenied => UpdateError::PermissionDenied {
                path: path.to_path_buf(),
                source: err,
            },
            _ => UpdateError::Internal(format!("{ctx}: {err}")),
        }
    }

    fn finalize_target(_target: &Path) -> Result<(), UpdateError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(_target, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| UpdateError::Internal(format!("chmod target after persist: {e}")))?;
        }
        Ok(())
    }

    fn rollback_after_persist_failure(
        target: &Path,
        backup: &Path,
        tmp_path: &Path,
        replace_io: std::io::Error,
    ) -> UpdateError {
        let rollback_result = std::fs::rename(backup, target);
        let _ = std::fs::remove_file(tmp_path);
        match rollback_result {
            Ok(()) => UpdateError::ReplaceFailedRolledBack { source: replace_io },
            Err(rollback_io) => {
                tracing::error!(
                    target = %target.display(),
                    backup = %backup.display(),
                    replace_error = %replace_io,
                    rollback_error = %rollback_io,
                    "rollback failed: original may be at .bak path"
                );
                UpdateError::RollbackFailed {
                    replace_source: replace_io,
                    rollback_source: rollback_io,
                }
            }
        }
    }
}

impl Default for AtomicBinaryReplacer {
    fn default() -> Self {
        Self::new()
    }
}

impl BinaryReplacer for AtomicBinaryReplacer {
    fn replace(&self, target: &Path, new_bytes: &[u8]) -> Result<ReplaceOutcome, UpdateError> {
        let _lock = UpdateLock::acquire(&sibling_with_suffix(target, LOCK_SUFFIX))?;

        let parent = target
            .parent()
            .ok_or_else(|| UpdateError::Internal(format!("target has no parent: {target:?}")))?;
        let backup = sibling_with_suffix(target, BACKUP_SUFFIX);

        let tmp = Self::write_temp(parent, new_bytes)?;
        let tmp_path = tmp.path().to_path_buf();

        std::fs::rename(target, &backup)
            .map_err(|e| Self::map_rename_err(e, target, "backing up original"))?;

        let persist_err = match tmp.persist(target) {
            Ok(_persisted) => {
                Self::finalize_target(target)?;
                return Ok(ReplaceOutcome {
                    backup_path: backup,
                });
            }
            Err(e) => e,
        };
        Err(Self::rollback_after_persist_failure(
            target,
            &backup,
            &tmp_path,
            persist_err.error,
        ))
    }
}

// ----------------------------------------------------------------------------
// FakeReplacer — module-scope under #[cfg(test)] so installer.rs tests can use it.
// ----------------------------------------------------------------------------

#[cfg(test)]
pub(crate) enum ReplacerBehavior {
    Succeed { backup_path: PathBuf },
    Fail(UpdateError),
}

#[cfg(test)]
impl ReplacerBehavior {
    fn into_outcome(self) -> Result<ReplaceOutcome, UpdateError> {
        match self {
            Self::Succeed { backup_path } => Ok(ReplaceOutcome { backup_path }),
            Self::Fail(err) => Err(err),
        }
    }
}

#[cfg(test)]
pub(crate) struct ReplacerCall {
    pub target: PathBuf,
    pub bytes_len: usize,
}

#[cfg(test)]
pub(crate) struct FakeReplacer {
    queue: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<ReplacerBehavior>>>,
    pub calls: std::sync::Arc<std::sync::Mutex<Vec<ReplacerCall>>>,
}

#[cfg(test)]
impl FakeReplacer {
    pub(crate) fn new(behaviors: impl IntoIterator<Item = ReplacerBehavior>) -> Self {
        Self {
            queue: std::sync::Arc::new(std::sync::Mutex::new(behaviors.into_iter().collect())),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn call_count(&self) -> usize {
        self.calls.lock().expect("calls mutex").len()
    }
}

#[cfg(test)]
impl BinaryReplacer for FakeReplacer {
    fn replace(&self, target: &Path, new_bytes: &[u8]) -> Result<ReplaceOutcome, UpdateError> {
        self.calls.lock().expect("calls mutex").push(ReplacerCall {
            target: target.to_path_buf(),
            bytes_len: new_bytes.len(),
        });
        let next = self
            .queue
            .lock()
            .expect("queue mutex")
            .pop_front()
            .unwrap_or_else(|| {
                panic!("FakeReplacer: no behaviors remaining — test over-called replace()")
            });
        next.into_outcome()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_replacer_round_trip_writes_bytes_and_sets_exec_permission() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("maestro");
        std::fs::write(&dest, b"old binary").expect("write original");

        let replacer = AtomicBinaryReplacer::new();
        let result = replacer.replace(&dest, b"new binary content");

        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        let outcome = result.expect("outcome");

        let on_disk = std::fs::read(&dest).expect("read dest");
        assert_eq!(on_disk, b"new binary content");

        let backup = std::fs::read(&outcome.backup_path).expect("read backup");
        assert_eq!(backup, b"old binary");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dest)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                0o755,
                "expected 0o755, got {:o}",
                mode & 0o777
            );
        }
    }

    #[test]
    fn atomic_replacer_leaves_no_tmp_file_on_success() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("maestro");
        std::fs::write(&dest, b"original").expect("write original");

        let replacer = AtomicBinaryReplacer::new();
        replacer.replace(&dest, b"new bytes").expect("replace ok");

        let stray: Vec<_> = std::fs::read_dir(dir.path())
            .expect("readdir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.contains(".tmp")
            })
            .collect();
        assert!(
            stray.is_empty(),
            "stray tmp files found: {:?}",
            stray.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacer_permission_denied_returns_typed_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("maestro");
        std::fs::write(&dest, b"old").expect("write original");

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod 0o555");

        let replacer = AtomicBinaryReplacer::new();
        let result = replacer.replace(&dest, b"new bytes");

        let _ = std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755));

        assert!(
            matches!(result, Err(UpdateError::PermissionDenied { .. })),
            "expected PermissionDenied, got: {result:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacer_leaves_no_partial_file_on_permission_failure() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("maestro");
        std::fs::write(&dest, b"old").expect("write original");

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod 0o555");

        let replacer = AtomicBinaryReplacer::new();
        let _ = replacer.replace(&dest, b"new bytes");

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
            .expect("chmod restore");

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("readdir")
            .filter_map(|e| e.ok())
            .collect();

        for entry in &entries {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            assert!(
                s == "maestro" || s == "maestro.bak",
                "unexpected file after permission failure: {s}"
            );
        }
    }
}
