use super::App;
use super::helpers::session_label;
use super::types::TuiMode;
use crate::gates::runner::{self, GateCheck, GateRunner};
use crate::gates::types::{CompletionGate, GateResult};
use crate::git::GitOps;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::retry::RetryPolicy;
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;
use std::time::{Duration, Instant};

impl App {
    pub async fn check_completions(&mut self) -> anyhow::Result<()> {
        // Fire pending plugin hooks
        let pending_hooks = std::mem::take(&mut self.pending_hooks);
        for ph in pending_hooks {
            self.fire_plugin_hook(ph.hook, ph.ctx).await;
        }

        // Process pending PR retries
        self.process_pending_pr_retries().await;

        // Process pending issue completions (gates, git push, label updates, PR creation)
        let pending = std::mem::take(&mut self.pending_issue_completions);

        // Build gates once from config (independent of individual completions)
        let gates: Vec<CompletionGate> = if let Some(ref cfg) = self.config
            && cfg.sessions.completion_gates.enabled
            && !cfg.sessions.completion_gates.commands.is_empty()
        {
            cfg.sessions
                .completion_gates
                .commands
                .iter()
                .map(CompletionGate::from_config_entry)
                .collect()
        } else if let Some(ref cfg) = self.config
            && cfg.gates.enabled
        {
            vec![CompletionGate::TestsPass {
                command: cfg.gates.test_command.clone(),
            }]
        } else {
            vec![]
        };

        for mut completion in pending {
            let issue_label = format!("#{}", completion.issue_number);

            // Run completion gates before accepting the result
            if completion.success
                && !gates.is_empty()
                && let Some(wt_path) = &completion.worktree_path
            {
                if let Some(managed) = self.pool.find_by_issue_mut(completion.issue_number) {
                    let _ = managed
                        .session
                        .transition_to(SessionStatus::GatesRunning, TransitionReason::GatesStarted);
                }

                let gate_runner = GateRunner;
                let results = gate_runner.run_gates(&gates, wt_path);

                let paired: Vec<(GateResult, bool)> = results
                    .into_iter()
                    .zip(gates.iter().map(|g| g.is_required()))
                    .collect();

                let all_required_passed = runner::all_required_gates_passed(&paired);

                // Log individual gate results (failed-level differs by outcome)
                let fail_level = if all_required_passed {
                    LogLevel::Warn
                } else {
                    LogLevel::Error
                };
                for (result, _) in &paired {
                    let level = if result.passed {
                        LogLevel::Info
                    } else {
                        fail_level
                    };
                    self.activity_log.push_simple(
                        issue_label.clone(),
                        format!("Gate [{}]: {}", result.gate, result.message),
                        level,
                    );
                }

                if !all_required_passed {
                    let failures: Vec<String> = paired
                        .iter()
                        .filter(|(r, required)| !r.passed && *required)
                        .map(|(r, _)| r.message.clone())
                        .collect();

                    let failed_gate_results: Vec<crate::session::types::GateResultEntry> = paired
                        .iter()
                        .filter(|(r, _)| !r.passed)
                        .map(|(r, _)| crate::session::types::GateResultEntry {
                            gate: r.gate.clone(),
                            passed: r.passed,
                            message: r.message.clone(),
                        })
                        .collect();

                    if let Some(managed) = self.pool.find_by_issue_mut(completion.issue_number) {
                        managed.session.gate_results = failed_gate_results;
                        let _ = managed.session.transition_to(
                            SessionStatus::NeedsReview,
                            TransitionReason::GatesFailed,
                        );
                        managed
                            .session
                            .log_activity(format!("Gates failed: {}", failures.join("; ")));
                    }

                    completion.success = false;
                    let ctx = HookContext::new()
                        .with_session("", Some(completion.issue_number))
                        .with_var("MAESTRO_GATE_FAILURES", &failures.join("; "));
                    self.fire_plugin_hook(HookPoint::TestsFailed, ctx).await;
                } else {
                    self.activity_log.push_simple(
                        issue_label.clone(),
                        "All required gates passed".into(),
                        LogLevel::Info,
                    );
                    let ctx = HookContext::new().with_session("", Some(completion.issue_number));
                    self.fire_plugin_hook(HookPoint::TestsPassed, ctx).await;
                }
            }

            // If successful and we have a worktree, commit and push changes
            if completion.success
                && let (Some(branch), Some(wt_path)) =
                    (&completion.worktree_branch, &completion.worktree_path)
            {
                let git_ops = crate::git::CliGitOps;
                let commit_msg = if completion.issue_numbers.len() >= 2 {
                    let refs: Vec<String> = completion
                        .issue_numbers
                        .iter()
                        .map(|n| format!("#{}", n))
                        .collect();
                    format!("feat: implement unified changes for {}", refs.join(", "))
                } else {
                    format!(
                        "feat: implement changes for issue #{}",
                        completion.issue_number
                    )
                };
                match git_ops.commit_and_push(wt_path, branch, &commit_msg) {
                    Ok(()) => {
                        self.activity_log.push_simple(
                            format!("#{}", completion.issue_number),
                            format!("Pushed to branch {}", branch),
                            LogLevel::Info,
                        );
                    }
                    Err(e) => {
                        self.activity_log.push_simple(
                            format!("#{}", completion.issue_number),
                            format!("Git push failed: {}", e),
                            LogLevel::Error,
                        );
                    }
                }
            }

            self.on_issue_session_completed(
                completion.issue_number,
                completion.issue_numbers,
                completion.success,
                completion.cost_usd,
                completion.files_touched,
                completion.worktree_branch,
                completion.worktree_path,
                completion.is_ci_fix,
            )
            .await;
        }

        // Stall detection: check for sessions that haven't produced events
        let stall_timeout = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.sessions.stall_timeout_secs))
            .unwrap_or(Duration::from_secs(300));

        let stalled_ids = self.health_monitor.check_stalls(stall_timeout);
        for id in &stalled_ids {
            if let Some(managed) = self.pool.get_active_mut(*id)
                && managed.session.status == SessionStatus::Running
            {
                let _ = managed
                    .session
                    .transition_to(SessionStatus::Stalled, TransitionReason::HealthStall);
                let label = session_label(&managed.session);
                self.activity_log.push_simple(
                    label,
                    format!(
                        "Session stalled (no activity for {}s)",
                        stall_timeout.as_secs()
                    ),
                    LogLevel::Error,
                );
            }
        }

        // Retry eligible sessions (stalled or errored) before finalizing
        let retry_policy = self
            .config
            .as_ref()
            .map(|c| RetryPolicy::from_config(&c.sessions));

        let retryable_ids: Vec<uuid::Uuid> = if let Some(ref policy) = retry_policy {
            self.pool
                .all_sessions()
                .iter()
                .filter(|s| policy.should_retry(s))
                .map(|s| s.id)
                .collect()
        } else {
            Vec::new()
        };

        let consultation_skip_ids: Vec<uuid::Uuid> = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| {
                s.status == SessionStatus::Completed && RetryPolicy::is_consultation_satisfied(s)
            })
            .map(|s| s.id)
            .collect();

        for id in &consultation_skip_ids {
            if let Some(managed) = self.pool.get_active_mut(*id)
                && !managed.session.consultation_skip_logged
            {
                let label = session_label(&managed.session);
                managed
                    .session
                    .log_activity("consultation prompt answered successfully".to_string());
                managed.session.consultation_skip_logged = true;
                self.activity_log.push_simple(
                    label,
                    "Skipping retry: consultation prompt answered successfully".to_string(),
                    LogLevel::Info,
                );
            }
        }

        let mut retry_sessions = Vec::new();
        for id in &retryable_ids {
            if let Some(policy) = &retry_policy
                && let Some(managed) = self.pool.get_active_mut(*id)
                && policy.should_retry(&managed.session)
            {
                let label = session_label(&managed.session);
                // Gather progress and last error for rich retry context
                let progress = self.progress_tracker.get(id).cloned();
                let last_error = managed
                    .session
                    .activity_log
                    .iter()
                    .rev()
                    .find(|e| e.message.starts_with("ERROR:") || e.message.contains("failed"))
                    .map(|e| e.message.clone());
                let retry = policy.prepare_retry(
                    &managed.session,
                    progress.as_ref(),
                    last_error.as_deref(),
                );
                let reason = if managed.session.is_hollow_completion {
                    "hollow completion"
                } else {
                    "stalled/errored"
                };
                let max = policy.effective_max(&managed.session);
                let _ = managed
                    .session
                    .transition_to(SessionStatus::Retrying, TransitionReason::RetryTriggered);
                self.activity_log.push_simple(
                    label,
                    format!(
                        "Retrying (attempt {}/{}) — reason: {}",
                        retry.retry_count, max, reason
                    ),
                    LogLevel::Warn,
                );
                retry_sessions.push(retry);
            }
        }

        // Enqueue retry sessions
        for session in retry_sessions {
            self.add_session(session).await?;
        }

        if self.adapt_follow_up_screen.is_none() {
            // Pick the first just-completed session we haven't yet evaluated;
            // mark the candidate "considered" so the overlay sticks when
            // dismissed and `parse_suggestions` doesn't run twice per tick.
            let candidate_id = self
                .pool
                .all_sessions()
                .iter()
                .find(|s| {
                    matches!(s.status, SessionStatus::Completed | SessionStatus::Retrying)
                        && !s.adapt_follow_up_considered
                        && !s.last_message.trim().is_empty()
                })
                .map(|s| s.id);

            if let Some(id) = candidate_id
                && let Some(managed) = self.pool.get_active_mut(id)
            {
                managed.session.adapt_follow_up_considered = true;
                let suggestions =
                    crate::adapt::suggestions::parse_suggestions(&managed.session.last_message);
                if suggestions.len() >= 2 {
                    let label = session_label(&managed.session);
                    self.adapt_follow_up_screen = Some(
                        crate::tui::screens::AdaptFollowUpScreen::new(label, suggestions),
                    );
                    self.tui_mode = TuiMode::AdaptFollowUp;
                }
            }
        }

        // Show hollow retry prompt for sessions that exceeded auto-retry limits.
        // Consultation-satisfied sessions are excluded — they're already "done".
        if self.hollow_retry_screen.is_none()
            && let Some(hollow_session) = self.pool.all_sessions().iter().find(|s| {
                s.status == SessionStatus::Completed
                    && s.is_hollow_completion
                    && !retryable_ids.contains(&s.id)
                    && !RetryPolicy::is_consultation_satisfied(s)
            })
        {
            // This session wasn't retried (exceeded limits) — prompt user
            let label = session_label(hollow_session);
            let max = retry_policy
                .as_ref()
                .map(|p| p.effective_max(hollow_session))
                .unwrap_or(0);
            self.hollow_retry_screen = Some(crate::tui::screens::HollowRetryScreen::new(
                hollow_session.id,
                label,
                hollow_session.retry_count,
                max,
            ));
            self.tui_mode = TuiMode::HollowRetry;
        }

        // Find terminal sessions in the active list (including Retrying which is now done)
        let completed_ids: Vec<uuid::Uuid> = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| s.status.is_terminal() || s.status == SessionStatus::Retrying)
            .map(|s| s.id)
            .collect();

        // Only process sessions that are actually in the active list
        for id in &completed_ids {
            if self.pool.get_active_mut(*id).is_some() {
                self.pool.on_session_completed(*id);
                self.health_monitor.remove(*id);
                self.progress_tracker.remove(id);
            }
        }

        // Medium tier: work assigner tick (every ~10s)
        let work_tick_interval = self
            .config
            .as_ref()
            .map(|c| Duration::from_secs(c.monitoring.work_tick_interval_secs))
            .unwrap_or(Duration::from_secs(10));

        if self.last_work_tick.elapsed() >= work_tick_interval {
            self.last_work_tick = Instant::now();
            // Tick the work assigner to fill available slots from GitHub issues
            self.tick_work_assigner().await?;
        }

        // Slow tier: CI status polling (every ~30s)
        self.poll_ci_status();

        // Try to promote queued sessions
        let promoted_ids = self.pool.try_promote();
        if !promoted_ids.is_empty() {
            let tx = self.pool.event_tx();
            for id in promoted_ids {
                if let Some(managed) = self.pool.get_active_mut(id) {
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label.clone(),
                        "Spawning session...".into(),
                        LogLevel::Info,
                    );
                    if let Err(e) = managed.spawn(tx.clone()).await {
                        self.activity_log.push_simple(
                            label,
                            format!("Spawn failed: {}", e),
                            LogLevel::Error,
                        );
                    } else {
                        self.activity_log.push_simple(
                            label,
                            "Session started".into(),
                            LogLevel::Info,
                        );
                    }
                }
            }
        }

        self.sync_state();
        Ok(())
    }
}
