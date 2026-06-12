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
use crate::state::file_claims::ClaimResult;
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
}
