//! Auto-review hand-off from `/pushup` to a running maestro TUI.
//!
//! `/pushup` writes `~/.maestro/last-pr-created` after `gh pr create`
//! succeeds. Maestro polls the file once per `check_completions` tick;
//! on a fresh write it enqueues `TuiCommand::PrCreated` (the same
//! command emitted by the in-session PR-URL detector at
//! `event_handler.rs`) and deletes the marker so it is consumed once.
//!
//! Marker shape:
//! ```json
//! {"pr_number": 123, "owner": "owner", "repo": "repo", "ts": "..."}
//! ```
//! `ts` is informational and is not parsed.
//!
//! Failure modes:
//! - Marker absent → silent no-op.
//! - Marker mtime equals last-seen mtime → no-op (avoids re-firing).
//! - Marker is a symlink → Warn-log, unlink the symlink (NOT the
//!   target), no command queued.
//! - Marker contains malformed JSON or fails the owner/repo guard →
//!   Warn-log, delete the file, no command queued.

use super::App;
use crate::tui::activity_log::LogLevel;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MARKER_REL_PATH: &str = ".maestro/last-pr-created";

#[derive(serde::Deserialize)]
struct PushupMarker {
    pr_number: u64,
    owner: String,
    repo: String,
}

impl App {
    fn home_dir(&self) -> Option<PathBuf> {
        if let Some(ref override_path) = self.home_dir_override {
            return Some(override_path.clone());
        }
        std::env::var_os("HOME").map(PathBuf::from)
    }

    fn marker_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(MARKER_REL_PATH))
    }

    /// Unlink the marker and reset the cached mtime. Errors from
    /// `remove_file` are intentionally swallowed: the cleanup is best-
    /// effort, and the next tick will re-attempt if the marker is still
    /// there. `remove_file` does NOT follow symlinks (it `unlink`s the
    /// link itself), so this is safe to call on a symlinked marker.
    fn consume_marker(&mut self, path: &Path) {
        let _ = std::fs::remove_file(path);
        self.last_pr_marker_mtime = None;
    }

    /// Poll `~/.maestro/last-pr-created`; on a fresh marker enqueue
    /// `TuiCommand::PrCreated` and delete the file. Called once per
    /// `check_completions` tick.
    pub async fn poll_last_pr_created_marker(&mut self) {
        let Some(path) = self.marker_path() else {
            return;
        };
        // symlink_metadata so we detect a symlink BEFORE read_to_string
        // follows it.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            return;
        };
        if meta.file_type().is_symlink() {
            self.activity_log.push_simple(
                "PUSHUP".into(),
                format!(
                    "Refusing to read ~/.maestro/last-pr-created: it is a symlink at {:?}",
                    path
                ),
                LogLevel::Warn,
            );
            self.consume_marker(&path);
            return;
        }
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if Some(mtime) == self.last_pr_marker_mtime {
            return;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => {
                self.last_pr_marker_mtime = Some(mtime);
                return;
            }
        };
        let marker = match serde_json::from_str::<PushupMarker>(&raw) {
            Ok(m) => m,
            Err(e) => {
                self.activity_log.push_simple(
                    "PUSHUP".into(),
                    format!(
                        "Could not parse ~/.maestro/last-pr-created: {} — deleting",
                        e
                    ),
                    LogLevel::Warn,
                );
                self.consume_marker(&path);
                return;
            }
        };
        if let Err(e) = validate_marker_owner_repo(&marker) {
            self.activity_log.push_simple(
                "PUSHUP".into(),
                format!(
                    "Marker owner/repo rejected: {} — deleting (security guard)",
                    e
                ),
                LogLevel::Warn,
            );
            self.consume_marker(&path);
            return;
        }
        self.activity_log.push_simple(
            "PUSHUP".into(),
            format!(
                "Detected /pushup PR #{}; dispatching auto-review",
                marker.pr_number
            ),
            LogLevel::Info,
        );
        self.pending_commands
            .push(super::types::TuiCommand::PrCreated {
                pr_number: marker.pr_number,
                owner: marker.owner,
                repo: marker.repo,
            });
        self.consume_marker(&path);
    }
}

/// Defense-in-depth: even though `~/.maestro/` is per-user, a same-user
/// attacker who plants a marker should not be able to redirect maestro's
/// auto-review to a `gh pr view --repo "../other-org/repo"`. Reject
/// anything that would not survive `validate_gh_arg` or fails the
/// no-slashes check on either field.
fn validate_marker_owner_repo(marker: &PushupMarker) -> anyhow::Result<()> {
    crate::util::validate_gh_arg(&marker.owner, "marker owner")?;
    crate::util::validate_gh_arg(&marker.repo, "marker repo")?;
    if marker.owner.contains('/') || marker.repo.contains('/') {
        anyhow::bail!(
            "marker owner/repo must not contain slashes (got owner={:?}, repo={:?})",
            marker.owner,
            marker.repo
        );
    }
    Ok(())
}
