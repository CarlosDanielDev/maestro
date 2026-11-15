//! Typed wrapper around the `~/.maestro/last-pr-created` hand-off marker.
//!
//! `/pushup` writes this marker after `gh pr create`; a running maestro
//! polls it (`src/tui/app/pushup_marker.rs`) and enqueues
//! `TuiCommand::PrCreated`. `issue_number` (added #735) lets the future
//! terminator path (#739) match the PR to an active interaction session.
//! Legacy markers written before the schema change have no `issue_number`;
//! they are tolerated — `read` logs a warn and the caller falls through to
//! the plain `PrCreated` path.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrMarker {
    pub pr_number: u64,
    pub owner: String,
    pub repo: String,
    /// Added #735. `None` for legacy markers; the reader logs a warn and
    /// the caller falls through to the plain `PrCreated` path.
    #[serde(default)]
    pub issue_number: Option<u64>,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum MarkerError {
    #[error("failed to read marker at {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write marker at {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("malformed marker JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

impl PrMarker {
    /// `.tmp` staging path: append `.tmp` to the full file name (matches
    /// the shell writer's `${marker}.tmp`, NOT `with_extension`).
    fn tmp_path(path: &Path) -> PathBuf {
        let mut staged = path.to_path_buf().into_os_string();
        staged.push(".tmp");
        staged.into()
    }

    /// Atomic write: serialize to `<path>.tmp`, then `rename` over `path`.
    /// `rename` within the same directory is atomic on POSIX, so a
    /// concurrent reader never observes a half-written marker.
    pub fn write_atomic(&self, path: &Path) -> Result<(), MarkerError> {
        let tmp = Self::tmp_path(path);
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        std::fs::write(&tmp, json.as_bytes()).map_err(|source| MarkerError::Write {
            path: tmp.display().to_string(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| MarkerError::Write {
            path: path.display().to_string(),
            source,
        })?;
        Ok(())
    }

    /// Read + parse. Tolerant of a missing `issue_number` (serde default
    /// -> `None`). Logs a warn at the seam when the field is absent so the
    /// caller knows the interaction-session match is impossible.
    pub fn read(path: &Path) -> Result<Self, MarkerError> {
        let raw = std::fs::read_to_string(path).map_err(|source| MarkerError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let marker: PrMarker = serde_json::from_str(&raw)?;
        if marker.issue_number.is_none() {
            tracing::warn!(
                pr_number = marker.pr_number,
                "marker missing issue_number; cannot match interaction session; \
                 falling through to PrCreated path"
            );
        }
        Ok(marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_missing_issue_number_gives_none() {
        let json = r#"{"pr_number":1,"owner":"o","repo":"r","ts":"2024-06-01T00:00:00Z"}"#;
        let m: PrMarker = serde_json::from_str(json).expect("must parse");
        assert_eq!(m.issue_number, None);
    }

    #[test]
    fn deserialize_present_issue_number_gives_some() {
        let json = r#"{"pr_number":1,"owner":"o","repo":"r","issue_number":42,"ts":"2024-06-01T00:00:00Z"}"#;
        let m: PrMarker = serde_json::from_str(json).expect("must parse");
        assert_eq!(m.issue_number, Some(42));
    }
}
