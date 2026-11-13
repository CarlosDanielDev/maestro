//! File-claim conflict handling for mutating tool events. Split out of
//! `event_handler.rs` (file-size budget): `handle_session_event` calls
//! [`App::process_file_claim_event`] before the per-session machinery runs.

use super::App;
use super::types::PendingHook;
use crate::config::ConflictPolicy;
use crate::notifications::slack::SlackEvent;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::transition::TransitionReason;
use crate::session::types::{SessionStatus, StreamEvent};
use crate::state::file_claims::{ClaimResult, repo_relative_key};
use crate::tui::activity_log::LogLevel;

impl App {
    /// Claim the file behind a mutating `ToolUse` (`Write`/`Edit`) for
    /// `session_id` and run the configured conflict policy on a clash:
    /// log + notify, then warn/pause/kill per `sessions.conflict.policy`.
    pub(super) fn process_file_claim_event(&mut self, session_id: uuid::Uuid, event: &StreamEvent) {
        let StreamEvent::ToolUse {
            ref tool,
            file_path: Some(ref path),
            ..
        } = *event
        else {
            return;
        };
        if !matches!(tool.as_str(), "Write" | "Edit") {
            return;
        }

        // #1156: key on the repo-relative path — each session writes inside
        // its own worktree, so raw paths never collide across sessions.
        let worktree = self
            .pool
            .get_active_mut(session_id)
            .and_then(|m| m.worktree_path.clone());
        let key = repo_relative_key(path, worktree.as_deref());
        let path = key.as_str();
        let result = self.pool.file_claims.claim(path, session_id);
        if let ClaimResult::Conflict { owner } = result {
            let label = format!("S-{}", &session_id.to_string()[..8]);
            let owner_short = &owner.to_string()[..8];

            self.pool
                .file_claims
                .record_conflict(path, owner, session_id);

            self.activity_log.push_simple(
                label,
                format!("CONFLICT: {} claimed by S-{}", path, owner_short),
                LogLevel::Error,
            );

            self.notifications.notify(
                crate::notifications::types::InterruptLevel::Critical,
                "File Conflict",
                &format!(
                    "S-{} tried to write {} (owned by S-{})",
                    &session_id.to_string()[..8],
                    path,
                    owner_short
                ),
            );
            self.notifications.notify_slack(SlackEvent::FileConflict {
                file_path: path.to_string(),
                sessions: vec![session_id.to_string(), owner.to_string()],
            });

            let policy = self
                .config
                .as_ref()
                .map(|c| c.sessions.conflict.policy)
                .unwrap_or(ConflictPolicy::Warn);

            match policy {
                ConflictPolicy::Warn => {}
                ConflictPolicy::Pause => {
                    #[cfg(unix)]
                    if let Some(managed) = self.pool.get_active_mut(session_id) {
                        let _ = managed.pause();
                        let _ = managed
                            .session
                            .transition_to(SessionStatus::Paused, TransitionReason::ConflictPolicy);
                        managed
                            .session
                            .log_activity(format!("Paused due to conflict on {}", path));
                        self.activity_log.push_simple(
                            format!("S-{}", &session_id.to_string()[..8]),
                            format!("Session paused (conflict policy) on {}", path),
                            LogLevel::Warn,
                        );
                    }
                }
                ConflictPolicy::Kill => {
                    if let Some(managed) = self.pool.get_active_mut(session_id) {
                        let _ = managed
                            .session
                            .transition_to(SessionStatus::Killed, TransitionReason::ConflictPolicy);
                        managed
                            .session
                            .log_activity(format!("Killed due to conflict on {}", path));
                        self.activity_log.push_simple(
                            format!("S-{}", &session_id.to_string()[..8]),
                            format!("Session killed (conflict policy) on {}", path),
                            LogLevel::Error,
                        );
                    }
                }
            }

            self.pending_hooks.push(PendingHook {
                hook: HookPoint::FileConflict,
                ctx: HookContext::new()
                    .with_session(&session_id.to_string(), None)
                    .with_var("MAESTRO_CONFLICT_FILE", path)
                    .with_var("MAESTRO_CONFLICT_OWNER", &owner.to_string())
                    .with_var("MAESTRO_CONFLICT_POLICY", policy.label()),
            });
        }
    }

    /// Log one line per earlier PR from this run that changes any of the
    /// same files as the newly opened `pr_number` (#1156).
    pub(super) fn report_prs_sharing_files(
        &mut self,
        issue: u64,
        pr_number: u64,
        files: &[String],
    ) {
        for (earlier, shared) in self.pool.file_claims.record_pr_files(pr_number, files) {
            self.activity_log.push_simple(
                format!("#{}", issue),
                format!(
                    "PRs #{} and #{} both change {}",
                    earlier,
                    pr_number,
                    shared.join(", ")
                ),
                LogLevel::Warn,
            );
        }
    }
}

#[cfg(test)]
#[path = "claim_conflicts_tests.rs"]
mod tests;
