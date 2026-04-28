use crate::updater::error::UpdateError;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// RAII guard for the binary update flow.
///
/// Holds an exclusive flock on a per-target sentinel file (`<target>.update.lock`).
/// On drop, the kernel releases the flock when the underlying `File` closes; we
/// also remove the sentinel file best-effort so a clean exit leaves no breadcrumb.
#[derive(Debug)]
pub(crate) struct UpdateLock {
    path: PathBuf,
    _file: File,
}

impl UpdateLock {
    /// Try to acquire the lock at `path`.
    ///
    /// Returns `UpdateError::ConcurrentUpdate` if another holder has it,
    /// `UpdateError::PermissionDenied` if the lock file cannot be opened
    /// for permission reasons, and `UpdateError::Internal` for everything else.
    pub fn acquire(path: &Path) -> Result<Self, UpdateError> {
        // O_NOFOLLOW refuses a symlinked lock path so a co-located attacker
        // cannot trick us into writing PID bytes into an attacker-chosen file.
        let mut opts = OpenOptions::new();
        opts.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            opts.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = opts.open(path).map_err(|e| {
            #[cfg(unix)]
            if e.raw_os_error() == Some(libc::ELOOP) {
                return UpdateError::Internal(format!(
                    "refusing to open symlinked lock file at {}",
                    path.display()
                ));
            }
            match e.kind() {
                std::io::ErrorKind::PermissionDenied => UpdateError::PermissionDenied {
                    path: path.to_path_buf(),
                    source: e,
                },
                _ => UpdateError::Internal(format!("opening lock {}: {e}", path.display())),
            }
        })?;

        match file.try_lock() {
            Ok(()) => {
                use std::io::Write;
                let _ = (&file).write_all(format!("{}\n", std::process::id()).as_bytes());
                Ok(Self {
                    path: path.to_path_buf(),
                    _file: file,
                })
            }
            Err(std::fs::TryLockError::WouldBlock) => Err(UpdateError::ConcurrentUpdate {
                lock_path: path.to_path_buf(),
            }),
            Err(std::fs::TryLockError::Error(e)) => Err(UpdateError::Internal(format!(
                "try_lock {}: {e}",
                path.display()
            ))),
        }
    }
}

impl Drop for UpdateLock {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            tracing::warn!(
                path = %self.path.display(),
                error = %e,
                "failed to remove update lock file on drop"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn update_lock_acquire_succeeds_when_no_lock_exists() {
        let dir = tempdir().expect("tempdir");
        let lock_path = dir.path().join("maestro.update.lock");
        let result = UpdateLock::acquire(&lock_path);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        assert!(lock_path.exists(), "lock file should exist while held");
    }

    #[test]
    fn update_lock_second_acquire_returns_concurrent_update_error() {
        let dir = tempdir().expect("tempdir");
        let lock_path = dir.path().join("maestro.update.lock");
        let _lock1 = UpdateLock::acquire(&lock_path).expect("first acquire");
        let result = UpdateLock::acquire(&lock_path);
        assert!(
            matches!(result, Err(UpdateError::ConcurrentUpdate { .. })),
            "expected ConcurrentUpdate, got: {result:?}"
        );
    }

    #[test]
    fn update_lock_drop_releases_lock_and_allows_reacquire() {
        let dir = tempdir().expect("tempdir");
        let lock_path = dir.path().join("maestro.update.lock");
        {
            let _lock = UpdateLock::acquire(&lock_path).expect("first acquire");
        }
        let result = UpdateLock::acquire(&lock_path);
        assert!(result.is_ok(), "expected Ok after drop, got: {result:?}");
    }
}
