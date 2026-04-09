use super::App;
use super::helpers::session_label;
use super::types::PendingHook;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::fork::{ForkReason, ForkResult, SessionForker};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;

impl App {
    pub(super) fn check_context_overflow(&mut self, session_id: uuid::Uuid) {
        let Some(ref config) = self.config else {
            return;
        };
        let ctx_cfg = &config.sessions.context_overflow;

        if self
            .context_monitor
            .check_commit_prompt(session_id, ctx_cfg.commit_prompt_ratio())
        {
            self.context_monitor.mark_commit_prompted(session_id);
            let label = self
                .pool
                .get_active_mut(session_id)
                .map(|m| session_label(&m.session))
                .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
            self.activity_log.push_simple(
                label,
                format!(
                    "Context at {}%+ — consider committing work",
                    ctx_cfg.commit_prompt_pct
                ),
                LogLevel::Warn,
            );
        }

        if !self.flags.is_enabled(crate::flags::Flag::AutoFork) {
            return;
        }
        let overflow = self
            .context_monitor
            .check_overflow(session_id, ctx_cfg.overflow_ratio());
        let Some(overflow) = overflow else {
            return;
        };
        let Some(ref fork_policy) = self.fork_policy else {
            return;
        };

        let Some(managed) = self.pool.get_active_mut(session_id) else {
            return;
        };
        let parent_session = managed.session.clone();
        let progress = self.progress_tracker.get(&session_id);

        let fork_result = fork_policy.prepare_fork(
            &parent_session,
            progress,
            ForkReason::ContextOverflow {
                context_pct: overflow.context_pct,
            },
        );

        match fork_result {
            ForkResult::Forked { child, .. } => {
                let child_id = child.id;
                let label = session_label(&parent_session);

                self.activity_log.push_simple(
                    label,
                    format!(
                        "Context overflow at {:.0}% — forking to new session",
                        overflow.context_pct * 100.0
                    ),
                    LogLevel::Warn,
                );

                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    managed.session.child_session_ids.push(child_id);
                }
                self.state.record_fork(session_id, child_id);
                self.pool.enqueue(*child);

                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::ContextOverflow,
                    ctx: HookContext::new()
                        .with_session(&session_id.to_string(), parent_session.issue_number)
                        .with_var("MAESTRO_FORK_CHILD_ID", &child_id.to_string())
                        .with_var(
                            "MAESTRO_FORK_DEPTH",
                            &(parent_session.fork_depth + 1).to_string(),
                        ),
                });

                self.context_monitor.mark_overflow_triggered(session_id);

                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    let _ = managed
                        .session
                        .transition_to(SessionStatus::Completed, TransitionReason::ContextOverflow);
                    managed.session.current_activity = "Forked".into();
                    managed.session.log_activity(format!(
                        "Session forked to child {}",
                        &child_id.to_string()[..8]
                    ));
                }
            }
            ForkResult::Denied { reason } => {
                let label = self
                    .pool
                    .get_active_mut(session_id)
                    .map(|m| session_label(&m.session))
                    .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
                self.activity_log.push_simple(
                    label,
                    format!("Context overflow but fork denied: {}", reason),
                    LogLevel::Error,
                );
            }
        }
    }
}
