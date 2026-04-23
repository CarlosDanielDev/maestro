use super::App;
use super::helpers::session_label;
use super::types::PendingHook;
use crate::config::ConflictPolicy;
use crate::notifications::slack::SlackEvent;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::manager::SessionEvent;
use crate::session::transition::TransitionReason;
use crate::session::types::{SessionStatus, StreamEvent};
use crate::state::file_claims::{ClaimResult, FILE_CONFLICT_SENTINEL};
use crate::tui::activity_log::LogLevel;

impl App {
    /// Process a stream event from a session.
    pub fn handle_session_event(&mut self, evt: SessionEvent) {
        let session_id = evt.session_id;

        let _ = self.session_logger.log_event(session_id, &evt.event);
        self.health_monitor.record_activity(session_id);

        // File claim processing for mutating tools
        if let StreamEvent::ToolUse {
            ref tool,
            file_path: Some(ref path),
            ..
        } = evt.event
            && matches!(tool.as_str(), "Write" | "Edit")
        {
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
                            let _ = managed.session.transition_to(
                                SessionStatus::Paused,
                                TransitionReason::ConflictPolicy,
                            );
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
                            let _ = managed.session.transition_to(
                                SessionStatus::Killed,
                                TransitionReason::ConflictPolicy,
                            );
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

        // Sentinel detection
        if let StreamEvent::AssistantMessage { ref text } = evt.event
            && text.contains(FILE_CONFLICT_SENTINEL)
        {
            let label = format!("S-{}", &session_id.to_string()[..8]);
            self.activity_log.push_simple(
                label,
                "FILE_CONFLICT sentinel detected!".into(),
                LogLevel::Error,
            );
        }

        // Delegate event handling to pool's managed session
        if let Some(managed) = self.pool.get_active_mut(session_id) {
            managed.handle_event(&evt.event);
            let label = session_label(&managed.session);

            match &evt.event {
                StreamEvent::ToolUse {
                    tool,
                    file_path,
                    command_preview,
                    ..
                } => {
                    let detail = match (
                        tool.as_str(),
                        file_path.as_deref(),
                        command_preview.as_deref(),
                    ) {
                        ("Bash", _, Some(cmd)) => format!("$ {}", cmd),
                        (t, Some(path), _) => format!("{}: {}", t, path),
                        (t, None, _) => format!("Using {}", t),
                    };
                    self.activity_log
                        .push_tool(label, detail, LogLevel::Tool, tool.clone());
                    self.tool_start_times
                        .insert(session_id, (tool.clone(), std::time::Instant::now()));
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_tool_use(tool, file_path.as_deref());
                }
                StreamEvent::ToolResult { tool, is_error } => {
                    let duration_str = self
                        .tool_start_times
                        .remove(&session_id)
                        .map(|(_, start)| format!(" ({:.1}s)", start.elapsed().as_secs_f64()))
                        .unwrap_or_default();
                    let status = if *is_error { "FAILED" } else { "done" };
                    let detail = format!("{} {}{}", tool, status, duration_str);
                    let level = if *is_error {
                        LogLevel::Error
                    } else {
                        LogLevel::Tool
                    };
                    self.activity_log
                        .push_tool(label, detail, level, tool.clone());
                }
                StreamEvent::AssistantMessage { text } => {
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_message(text);
                }
                StreamEvent::Thinking { .. } => {}
                StreamEvent::TokenUpdate { .. } => {}
                StreamEvent::Completed { cost_usd } => {
                    self.activity_log.push_simple(
                        label.clone(),
                        format!("Completed (${:.2})", cost_usd),
                        LogLevel::Info,
                    );
                    if managed.session.is_hollow_completion {
                        self.activity_log.push_simple(
                            label,
                            "Hollow completion: session completed without performing any work"
                                .into(),
                            LogLevel::Warn,
                        );
                    }
                    self.notifications
                        .notify_slack(SlackEvent::SessionCompleted {
                            session_id: managed.session.id.to_string(),
                            issue_number: managed.session.issue_number,
                            cost_usd: *cost_usd,
                        });
                    self.pending_hooks.push(PendingHook {
                        hook: HookPoint::SessionCompleted,
                        ctx: HookContext::new()
                            .with_session(
                                &managed.session.id.to_string(),
                                managed.session.issue_number,
                            )
                            .with_cost(*cost_usd)
                            .with_files(&managed.session.files_touched),
                    });
                    // Update prompt history outcome
                    let outcome = if managed.session.is_hollow_completion {
                        crate::state::prompt_history::PromptOutcome::Hollow
                    } else {
                        crate::state::prompt_history::PromptOutcome::Completed
                    };
                    self.prompt_history
                        .update_outcome(managed.session.id, outcome);

                    if let Some(issue_num) = managed.session.issue_number {
                        self.pending_issue_completions
                            .push(super::types::PendingIssueCompletion {
                                issue_number: issue_num,
                                issue_numbers: managed.session.issue_numbers.clone(),
                                success: true,
                                cost_usd: *cost_usd,
                                files_touched: managed.session.files_touched.clone(),
                                worktree_branch: managed.branch_name.clone(),
                                worktree_path: managed.worktree_path.clone(),
                                is_ci_fix: managed.session.ci_fix_context.is_some(),
                            });
                    }
                }
                StreamEvent::Error { message } => {
                    self.activity_log.push_simple(
                        label,
                        format!("ERROR: {}", message),
                        LogLevel::Error,
                    );
                    self.prompt_history.update_outcome(
                        managed.session.id,
                        crate::state::prompt_history::PromptOutcome::Errored,
                    );
                    self.notifications.notify_slack(SlackEvent::SessionErrored {
                        session_id: managed.session.id.to_string(),
                        issue_number: managed.session.issue_number,
                        error: message.clone(),
                    });
                    if let Some(issue_num) = managed.session.issue_number {
                        self.pending_issue_completions
                            .push(super::types::PendingIssueCompletion {
                                issue_number: issue_num,
                                issue_numbers: managed.session.issue_numbers.clone(),
                                success: false,
                                cost_usd: managed.session.cost_usd,
                                files_touched: managed.session.files_touched.clone(),
                                worktree_branch: managed.branch_name.clone(),
                                worktree_path: managed.worktree_path.clone(),
                                is_ci_fix: managed.session.ci_fix_context.is_some(),
                            });
                    }
                }
                StreamEvent::ContextUpdate { context_pct } => {
                    self.context_monitor
                        .record_context(session_id, *context_pct);
                }
                _ => {}
            }
        }

        if matches!(evt.event, StreamEvent::ContextUpdate { .. }) {
            self.check_context_overflow(session_id);
        }

        self.check_budget(session_id);
        self.sync_state();
    }

    /// Route a bracketed-paste payload to the active screen.
    ///
    /// Embedded newlines are preserved as newline characters; the payload
    /// is never interpreted as a submit event. Screens without a text
    /// field fall through to a no-op.
    pub fn handle_paste(&mut self, text: &str) {
        tracing::debug!(paste_len = text.len(), "bracketed paste received");
        crate::tui::screen_dispatch::dispatch_paste_to_active_screen(self, text);
    }
}
