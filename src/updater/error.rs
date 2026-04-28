use std::path::PathBuf;
use thiserror::Error;

/// Typed error seam for the updater module.
///
/// Each variant maps to one user-facing message in the TUI banner / activity log.
/// The `Display` impl produces those strings verbatim.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(
        "permission denied — re-run with elevated privileges or move maestro to a writable path"
    )]
    PermissionDenied {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("network error during download: {message}")]
    NetworkInterrupted { message: String },

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("update failed — original version restored")]
    ReplaceFailedRolledBack { source: std::io::Error },

    #[error("update failed and rollback could not complete — please reinstall maestro manually")]
    RollbackFailed {
        replace_source: std::io::Error,
        rollback_source: std::io::Error,
    },

    #[error("another maestro instance is currently updating; please wait and try again")]
    ConcurrentUpdate { lock_path: PathBuf },

    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn update_error_display_permission_denied() {
        let err = UpdateError::PermissionDenied {
            path: PathBuf::from("/usr/local/bin/maestro"),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert_eq!(
            format!("{err}"),
            "permission denied — re-run with elevated privileges or move maestro to a writable path"
        );
    }

    #[test]
    fn update_error_display_network_interrupted() {
        let err = UpdateError::NetworkInterrupted {
            message: "connection reset by peer".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("network error during download"), "got: {msg}");
        assert!(msg.contains("connection reset by peer"), "got: {msg}");
    }

    #[test]
    fn update_error_display_replace_failed_rolled_back() {
        let err = UpdateError::ReplaceFailedRolledBack {
            source: io::Error::from(io::ErrorKind::Other),
        };
        assert_eq!(
            format!("{err}"),
            "update failed — original version restored"
        );
    }

    #[test]
    fn update_error_display_rollback_failed() {
        let err = UpdateError::RollbackFailed {
            replace_source: io::Error::from(io::ErrorKind::Other),
            rollback_source: io::Error::from(io::ErrorKind::BrokenPipe),
        };
        assert_eq!(
            format!("{err}"),
            "update failed and rollback could not complete — please reinstall maestro manually"
        );
    }

    #[test]
    fn update_error_display_concurrent_update() {
        let err = UpdateError::ConcurrentUpdate {
            lock_path: PathBuf::from("/tmp/maestro.update.lock"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("another maestro instance is currently updating"),
            "got: {msg}"
        );
    }
}
