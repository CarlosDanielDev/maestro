use super::App;
use super::helpers::session_label;
use super::types::PendingHook;
use crate::notifications::slack::SlackEvent;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::manager::SessionEvent;
use crate::session::types::StreamEvent;
use crate::state::file_claims::FILE_CONFLICT_SENTINEL;
use crate::tui::activity_log::LogLevel;

fn format_session_notify_title(prefix: &str, issue: Option<u64>, label: &str) -> String {
    match issue {
        Some(n) => format!("{}: #{} {}", prefix, n, label),
        None => format!("{}: {}", prefix, label),
    }
}

impl App {
    /// Process a stream event from a session.
    pub fn handle_session_event(&mut self, evt: SessionEvent) {
        let session_id = evt.session_id;

        let _ = self.session_logger.log_event(session_id, &evt.event);
        self.health_monitor.record_activity(session_id);

        // File claim processing for mutating tools (claim_conflicts.rs).
        self.process_file_claim_event(session_id, &evt.event);

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
                    subagent_name,
                } => {
                    let detail = if let Some(name) = subagent_name.as_deref() {
                        format!("Dispatching {}", name)
                    } else {
                        match (
                            tool.as_str(),
                            file_path.as_deref(),
                            command_preview.as_deref(),
                        ) {
                            ("Bash", _, Some(cmd)) => format!("$ {}", cmd),
                            (t, Some(path), _) => format!("{}: {}", t, path),
                            (t, None, _) => format!("Using {}", t),
                        }
                    };
                    self.activity_log.push_tool(
                        label,
                        detail,
                        LogLevel::Tool,
                        tool.clone(),
                        subagent_name.clone(),
                    );
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
                        .push_tool(label, detail, level, tool.clone(), None);
                }
                StreamEvent::AssistantMessage { text } => {
                    let progress = self.progress_tracker.get_or_create(session_id);
                    progress.on_message(text);
                    // PR auto-detect (#327): scan each line of assistant
                    // output for a GitHub PR URL. On hit, queue
                    // `TuiCommand::PrCreated` which triggers /review.
                    // Interactive sessions are exempt (#947): chat PRs go
                    // through the /pushup marker path (#739, reworked in
                    // Phase 4), not the one-shot auto-review trigger.
                    use crate::session::pr_capture::{GitHubPrUrlExtractor, PrUrlExtractor as _};
                    if managed.session.session_mode
                        != crate::session::types::SessionMode::Interactive
                    {
                        let extractor = GitHubPrUrlExtractor::new();
                        for line in text.lines() {
                            if let Some(evt) = extractor.extract(line) {
                                self.activity_log.push_simple(
                                    "PR".into(),
                                    format!("Detected PR #{}; triggering /review", evt.pr_number.0),
                                    LogLevel::Info,
                                );
                                self.pending_commands.push(
                                    crate::tui::app::TuiCommand::PrCreated {
                                        pr_number: evt.pr_number.0,
                                        owner: evt.owner,
                                        repo: evt.repo,
                                    },
                                );
                                break;
                            }
                        }
                    }
                }
                StreamEvent::Thinking { .. } => {}
                StreamEvent::TokenUpdate { .. } => {}
                StreamEvent::Completed { cost_usd } => {
                    let desktop_label = label.clone();
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
                    // Interactive settle (#947): the session stays alive
                    // for follow-ups — no completion pipeline (gates /
                    // auto-PR / teardown), no per-turn desktop/Slack/hook
                    // noise. The chat transcript line comes from
                    // forward_interactive_stream_event below.
                    if managed.session.session_mode
                        != crate::session::types::SessionMode::Interactive
                    {
                        let title = format_session_notify_title(
                            "Session complete",
                            managed.session.issue_number,
                            &desktop_label,
                        );
                        let body = format!(
                            "Cost ${:.2} — {} files changed",
                            cost_usd,
                            managed.session.files_touched.len()
                        );
                        self.desktop_notifier.notify(&title, &body);
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
                            self.pending_issue_completions.push(
                                super::types::PendingIssueCompletion {
                                    issue_number: issue_num,
                                    issue_numbers: managed.session.issue_numbers.clone(),
                                    success: true,
                                    cost_usd: *cost_usd,
                                    files_touched: managed.session.files_touched.clone(),
                                    worktree_branch: managed.branch_name.clone(),
                                    worktree_path: managed.worktree_path.clone(),
                                    is_ci_fix: managed.session.ci_fix_context.is_some(),
                                },
                            );
                        }
                    }
                }
                StreamEvent::Error { message } => {
                    let desktop_label = label.clone();
                    self.activity_log.push_simple(
                        label,
                        format!("ERROR: {}", message),
                        LogLevel::Error,
                    );
                    // Interactive failure stays alive for discuss + retry
                    // (#947, spec §4.3) — same exemption as Completed.
                    if managed.session.session_mode
                        != crate::session::types::SessionMode::Interactive
                    {
                        self.prompt_history.update_outcome(
                            managed.session.id,
                            crate::state::prompt_history::PromptOutcome::Errored,
                        );
                        let title = format_session_notify_title(
                            "Session errored",
                            managed.session.issue_number,
                            &desktop_label,
                        );
                        self.desktop_notifier.notify(&title, message);
                        self.notifications.notify_slack(SlackEvent::SessionErrored {
                            session_id: managed.session.id.to_string(),
                            issue_number: managed.session.issue_number,
                            error: message.clone(),
                        });
                        if let Some(issue_num) = managed.session.issue_number {
                            self.pending_issue_completions.push(
                                super::types::PendingIssueCompletion {
                                    issue_number: issue_num,
                                    issue_numbers: managed.session.issue_numbers.clone(),
                                    success: false,
                                    cost_usd: managed.session.cost_usd,
                                    files_touched: managed.session.files_touched.clone(),
                                    worktree_branch: managed.branch_name.clone(),
                                    worktree_path: managed.worktree_path.clone(),
                                    is_ci_fix: managed.session.ci_fix_context.is_some(),
                                },
                            );
                        }
                    }
                }
                StreamEvent::ContextUpdate { context_pct } => {
                    self.context_monitor
                        .record_context(session_id, *context_pct);
                }
                StreamEvent::Warning { code, message } => {
                    // `session_spawned` is a lifecycle marker carried on the
                    // Warning channel by the non-blocking spawn handshake
                    // (#803). It transitions Spawning → Running inside
                    // `ManagedSession::handle_event` above; suppressing it
                    // here keeps "WARNING [session_spawned]" out of the
                    // operator activity log where it would be misleading
                    // noise.
                    if code == crate::session::manager::SESSION_SPAWNED_CODE
                        || code == crate::session::manager::SESSION_BOUND_CODE
                    {
                        // no-op — already handled by managed.handle_event
                        // (session_bound binds the resume id, #947)
                    } else {
                        // Surface every other Warning in the activity log so
                        // operators see them even before the structured
                        // footer ships.
                        self.activity_log.push_simple(
                            label,
                            format!("WARNING [{code}]: {message}"),
                            LogLevel::Warn,
                        );
                        // Quota-forced bookkeeping for the home-screen
                        // footer badge (#845). Saturating to defend against
                        // pathological event floods.
                        if code == "quota_forced" {
                            self.minimax_forced_count_5h =
                                self.minimax_forced_count_5h.saturating_add(1);
                        }
                    }
                }
                _ => {}
            }
        }

        // Interactive-mode sessions additionally feed the chat transcript:
        // derive TurnEvents for the Interaction screen + persisted
        // view-model (#947). No-op for one-shot sessions.
        self.forward_interactive_stream_event(session_id, &evt.event);

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

#[cfg(test)]
#[path = "event_handler_tests.rs"]
mod tests;
