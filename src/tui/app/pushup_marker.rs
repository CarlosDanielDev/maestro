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
//! {"pr_number": 123, "owner": "owner", "repo": "repo", "issue_number": 703, "ts": "..."}
//! ```
//! Parsed via [`crate::work::pr_marker::PrMarker`]. `issue_number` (#735)
//! is optional: legacy markers without it are tolerated (a `tracing::warn`
//! fires) and still take the `PrCreated` path. `ts` is parsed but unused
//! here.
//!
//! Failure modes:
//! - Marker absent → silent no-op.
//! - Marker mtime equals last-seen mtime → no-op (avoids re-firing).
//! - Marker is a symlink → Warn-log, unlink the symlink (NOT the
//!   target), no command queued.
//! - Marker contains malformed JSON or fails the owner/repo guard →
//!   Warn-log, delete the file, no command queued.

use super::App;
use crate::session::interaction_lifecycle::InteractionLifecycleEvent;
use crate::tui::activity_log::LogLevel;
use crate::tui::screens::ScreenAction;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MARKER_REL_PATH: &str = ".maestro/last-pr-created";

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
        let marker = match crate::work::pr_marker::PrMarker::read(&path) {
            Ok(m) => m,
            Err(crate::work::pr_marker::MarkerError::Read { .. }) => {
                // Transient read failure: record mtime, retry next tick.
                self.last_pr_marker_mtime = Some(mtime);
                return;
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
        // #739: a marker carrying issue_number that matches an active
        // interaction session terminates that session. Runs BEFORE the
        // PrCreated enqueue so the single marker read drives both events.
        // owner/repo are cloned here because they are moved into PrCreated
        // below.
        if let Some(issue_number) = marker.issue_number
            && let Some(session) = self.pool.find_active_interaction_by_issue_mut(issue_number)
        {
            session.signal_terminator(
                crate::session::interaction_lifecycle::InteractionLifecycleEvent::PrLinkedToIssue {
                    pr_number: marker.pr_number,
                    issue_number,
                    owner: marker.owner.clone(),
                    repo: marker.repo.clone(),
                },
            );
            // #741: drive the open Interaction screen's terminator UI flow —
            // System turns + worktree teardown + auto-nav. The pool borrow ends
            // with this block, so the screen access below is a disjoint borrow.
            self.drive_interaction_terminator(
                issue_number,
                marker.pr_number,
                marker.owner.clone(),
                marker.repo.clone(),
            );
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

    /// Drive the open Interaction screen's terminator flow for `issue_number`
    /// (#741). Pushes the opening `INTERACTION closing` activity line, runs the
    /// screen's terminator handler (System turns + worktree teardown), and
    /// surfaces the returned `TEARDOWN` log line. No-op when the open screen is
    /// for a different issue or none is open. When the screen is mid-stream the
    /// teardown is deferred and its line is surfaced later from the
    /// `TurnFinished` path.
    fn drive_interaction_terminator(
        &mut self,
        issue_number: u64,
        pr_number: u64,
        owner: String,
        repo: String,
    ) {
        let Some(screen) = self.screen_state.interaction_screen.as_mut() else {
            return;
        };
        if !screen.is_for_issue(issue_number) {
            return;
        }
        let action = screen.on_terminator_signaled(InteractionLifecycleEvent::PrLinkedToIssue {
            pr_number,
            issue_number,
            owner,
            repo,
        });
        self.activity_log.push_simple(
            "INTERACTION".into(),
            format!("#{issue_number} closing (reason: PrCreated #{pr_number}); wiping worktree"),
            LogLevel::Info,
        );
        if let Some(ScreenAction::LogActivity {
            tag,
            message,
            level,
        }) = action
        {
            self.activity_log.push_simple(tag, message, level);
        }
        // #941: the blocking wipe never runs on the UI thread — take the
        // parked dispatch (if the terminator fired) and run it off-thread.
        self.spawn_pending_interaction_teardown();
    }

    /// Run a parked teardown dispatch under `spawn_blocking`, delivering the
    /// outcome back through `TuiDataEvent::InteractionTeardownResult` (#941).
    /// No-op when the open Interaction screen has nothing parked.
    pub(crate) fn spawn_pending_interaction_teardown(&mut self) {
        let Some(dispatch) = self
            .screen_state
            .interaction_screen
            .as_mut()
            .and_then(|screen| screen.take_pending_teardown_dispatch())
        else {
            return;
        };
        let tx = self.data_tx.clone();
        tokio::task::spawn_blocking(move || {
            use crate::tui::screens::interaction::lifecycle::{RealTeardown, WorktreeTeardownPort};
            let result = RealTeardown
                .wipe(
                    dispatch.issue_number,
                    &dispatch.path,
                    &dispatch.branch,
                    &dispatch.root,
                )
                .map_err(|err| err.to_string());
            let _ = tx.send(super::types::TuiDataEvent::InteractionTeardownResult {
                issue_number: dispatch.issue_number,
                result,
            });
        });
    }
}

/// Defense-in-depth: even though `~/.maestro/` is per-user, a same-user
/// attacker who plants a marker should not be able to redirect maestro's
/// auto-review to a `gh pr view --repo "../other-org/repo"`. Reject
/// anything that would not survive `validate_gh_arg` or fails the
/// no-slashes check on either field.
fn validate_marker_owner_repo(marker: &crate::work::pr_marker::PrMarker) -> anyhow::Result<()> {
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
