//! Auto-review hand-off from `/pushup` to a running maestro TUI (#545 P1).
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
//! - Marker absent  → silent no-op.
//! - Marker mtime equals last-seen mtime → no-op (avoids re-firing).
//! - Marker contains malformed JSON → Warn-log, delete the file, no
//!   command queued.

use super::App;
use crate::tui::activity_log::LogLevel;
use std::path::PathBuf;
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

    /// Poll `~/.maestro/last-pr-created`; on a fresh marker enqueue
    /// `TuiCommand::PrCreated` and delete the file. Called once per
    /// `check_completions` tick.
    pub async fn poll_last_pr_created_marker(&mut self) {
        let Some(path) = self.marker_path() else {
            return;
        };
        // Use symlink_metadata so a symlink at the marker path is detected
        // BEFORE we read it — read_to_string follows symlinks (security
        // review concern #8 on #545).
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
            // remove_file does NOT follow symlinks (it unlinks the link
            // itself), so this is safe — the link target stays intact.
            let _ = std::fs::remove_file(&path);
            self.last_pr_marker_mtime = None;
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
        match serde_json::from_str::<PushupMarker>(&raw) {
            Ok(marker) => {
                if let Err(e) = validate_marker_owner_repo(&marker) {
                    self.activity_log.push_simple(
                        "PUSHUP".into(),
                        format!(
                            "Marker owner/repo rejected: {} — deleting (security guard)",
                            e
                        ),
                        LogLevel::Warn,
                    );
                    let _ = std::fs::remove_file(&path);
                    self.last_pr_marker_mtime = None;
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
                let _ = std::fs::remove_file(&path);
                self.last_pr_marker_mtime = None;
            }
            Err(e) => {
                self.activity_log.push_simple(
                    "PUSHUP".into(),
                    format!(
                        "Could not parse ~/.maestro/last-pr-created: {} — deleting",
                        e
                    ),
                    LogLevel::Warn,
                );
                let _ = std::fs::remove_file(&path);
                self.last_pr_marker_mtime = None;
            }
        }
    }
}

/// Defense-in-depth: even though `~/.maestro/` is per-user, a same-user
/// attacker who plants a marker should not be able to redirect maestro's
/// auto-review to a `gh pr view --repo "../other-org/repo"` (security
/// review concern #8 on #545). Reject anything that would not survive
/// `validate_gh_arg` or fails the `owner/repo` shape.
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
