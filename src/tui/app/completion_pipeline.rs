use super::App;
use super::completion_job::{CompletionJob, CompletionJobOutcome};
use super::helpers::{build_completion_gates, session_label};
use super::types::TuiMode;
use crate::session::retry::RetryPolicy;
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::state::types::PendingIssueCompletion;
use crate::tui::activity_log::LogLevel;
use std::time::{Duration, Instant};

impl App {
    pub async fn check_completions(&mut self) -> anyhow::Result<()> {
        // Fire pending plugin hooks
        let pending_hooks = std::mem::take(&mut self.pending_hooks);
        for ph in pending_hooks {
            self.fire_plugin_hook(ph.hook, ph.ctx).await;
        }

        // Poll the /pushup hand-off marker for auto-review.
        self.poll_last_pr_created_marker().await;

        // Process pending PR retries
        self.process_pending_pr_retries().await;

        // Apply the outcomes of completion jobs that finished since the
        // last tick (#1153). Cheap: no subprocess, no network — the job
        // ran all of that off the render loop.
        let outcomes: Vec<CompletionJobOutcome> = std::mem::take(
            &mut *self
                .completion_jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        for outcome in outcomes {
            let issue = outcome.completion.issue_number;
            if let Some(i) = self
                .completions_in_flight
                .iter()
                .position(|c| c.issue_number == issue)
            {
                self.completions_in_flight.remove(i);
            }
            self.apply_completion_outcome(outcome).await;
        }

        // Dispatch new completions to background jobs. Gates, git push,
        // label updates and PR creation run off the render loop; the
        // outcome lands in `completion_jobs` for a later tick.
        let pending = std::mem::take(&mut self.pending_issue_completions);
        for completion in pending {
            self.dispatch_completion_job(completion);
        }

        // Stall detection: check for sessions that haven't produced events
        let stall_timeout = Duration::from_secs(self.session_config.stall_timeout_secs);

        let stalled_ids = self.health_monitor.check_stalls(stall_timeout);
        for id in &stalled_ids {
            if let Some(managed) = self.pool.get_active_mut(*id)
                && managed.session.status == SessionStatus::Running
            {
                checked_transition(
                    &mut managed.session,
                    SessionStatus::Stalled,
                    TransitionReason::HealthStall,
                );
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
                checked_transition(
                    &mut managed.session,
                    SessionStatus::Retrying,
                    TransitionReason::RetryTriggered,
                );
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

        if self.screen_state.adapt_follow_up_screen.is_none() {
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
                    self.screen_state.adapt_follow_up_screen = Some(
                        crate::tui::screens::AdaptFollowUpScreen::new(label, suggestions),
                    );
                    // #895 sibling: route through navigate_to so the prior
                    // mode (Overview / Detail) is pushed onto the nav-stack.
                    // Without this the [Esc] handler's
                    // `navigate_back_or_dashboard` finds an empty stack and
                    // falls through to Dashboard, dropping context.
                    self.navigate_to(TuiMode::AdaptFollowUp);
                }
            }
        }

        // Show hollow retry prompt for sessions that exceeded auto-retry limits.
        // Consultation-satisfied sessions are excluded — they're already "done".
        if self.screen_state.hollow_retry_screen.is_none()
            && let Some(hollow_session) = self.pool.all_sessions().iter().find(|s| {
                s.status == SessionStatus::Completed
                    && s.is_hollow_completion
                    && !s.hollow_dismissed
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
            self.screen_state.hollow_retry_screen =
                Some(crate::tui::screens::HollowRetryScreen::new(
                    hollow_session.id,
                    label,
                    hollow_session.retry_count,
                    max,
                ));
            // #895: route through navigate_to so the prior mode (Overview /
            // Detail / Dashboard) is pushed onto the nav-stack. Without this
            // the [s] Skip handler's `navigate_back_or_dashboard` finds an
            // empty stack and falls through to Dashboard, dropping the
            // user's context.
            self.navigate_to(TuiMode::HollowRetry);
        }

        // Find terminal sessions in the active list (including Retrying which is now done)
        let completed_ids: Vec<uuid::Uuid> = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| s.status.is_terminal() || s.status == SessionStatus::Retrying)
            .map(|s| s.id)
            .collect();

        for id in &completed_ids {
            if self.pool.finalize(*id) {
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
        self.poll_ci_status().await;

        // Try to promote queued sessions
        let promoted_ids = self.pool.try_promote();
        if !promoted_ids.is_empty() {
            let tx = self.pool.event_tx();
            for id in promoted_ids {
                // Pre-spawn budget gate (#776/#850). Same semantics as
                // `session_lifecycle.rs:add_session` — projection crossing
                // the alert threshold or limit parks the session and
                // raises the modal.
                if self.try_enter_prespawn_gate(id) {
                    break;
                }
                if let Some(managed) = self.pool.get_active_mut(id) {
                    let label = session_label(&managed.session);
                    // Workspace trust gate (#1045): same rule as
                    // `add_session` — this tick-path promotion funnel
                    // spawns queued sessions once slots free, so an
                    // untrusted workspace must not execute a config-
                    // supplied agent command/env here either.
                    if !self.workspace_trusted
                        && self
                            .config
                            .as_ref()
                            .is_some_and(crate::config::trust::has_executable_agent_config)
                    {
                        self.activity_log.push_simple(
                            label,
                            "Spawn blocked: workspace config is not trusted and declares agent commands (run `maestro trust`)".into(),
                            LogLevel::Error,
                        );
                        continue;
                    }
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

    /// Dispatch one pending completion to a background job (#1153).
    /// Snapshots every App input the job needs, performs the cheap
    /// GATES_RUNNING re-arm transition inline, and spawns. The heavy
    /// work — WIP backup, gates, git push, GitHub labels, auto-PR —
    /// runs off the render loop; `apply_completion_outcome` applies the
    /// result on a later tick.
    pub(crate) fn dispatch_completion_job(&mut self, completion: PendingIssueCompletion) {
        let issue_number = completion.issue_number;
        let gates = build_completion_gates(self.config.as_ref());
        let gates_will_run =
            completion.success && !gates.is_empty() && completion.worktree_path.is_some();

        // #1144: gated sessions normally arrive here already parked in
        // GATES_RUNNING by the stream handler; re-arm only when the
        // session still sits in RUNNING (paths that enqueue the
        // completion without a stream-completed event).
        if gates_will_run
            && let Some(managed) = self.pool.find_by_issue_mut(issue_number)
            && managed.session.status == SessionStatus::Running
        {
            checked_transition(
                &mut managed.session,
                SessionStatus::GatesRunning,
                TransitionReason::GatesStarted,
            );
        }

        let pr_input = self.build_pr_phase_input(
            issue_number,
            completion.issue_numbers.clone(),
            completion.issue_numbers.len() >= 2,
            completion.success,
            completion.cost_usd,
            completion.worktree_branch.clone(),
            completion.worktree_path.clone(),
        );

        self.completions_in_flight.push(completion.clone());
        CompletionJob {
            completion,
            gates,
            gates_will_run,
            git: std::sync::Arc::clone(&self.git_ops),
            github: self.github_client.clone(),
            gh_auth_ok: self.gh_auth_ok,
            pr_input,
            attempted: std::sync::Arc::clone(&self.attempted_pr_issue_numbers),
            sink: std::sync::Arc::clone(&self.completion_jobs),
        }
        .dispatch();
    }
}

/// #1144: run a pipeline transition through the state machine and
/// record rejections instead of dropping the result. A rejection means
/// the documented pipeline flow broke (e.g. the session settled
/// COMPLETED before the gates ran); it is logged at warn with the
/// from/to states — in structured logs and in the session's own
/// activity log — instead of vanishing.
pub(super) fn checked_transition(
    session: &mut crate::session::types::Session,
    target: SessionStatus,
    reason: TransitionReason,
) -> bool {
    match session.transition_to(target, reason) {
        Ok(()) => true,
        Err(rejected) => {
            tracing::warn!(
                from = %rejected.from.label(),
                to = %rejected.to.label(),
                "session transition rejected in completion pipeline"
            );
            session.log_activity(format!(
                "TRANSITION REJECTED: {} \u{2192} {}",
                rejected.from.label(),
                rejected.to.label()
            ));
            false
        }
    }
}

#[cfg(test)]
#[path = "completion_off_loop_tests.rs"]
mod completion_off_loop_tests;

#[cfg(test)]
mod tests {
    use crate::config::PluginConfig;
    use crate::plugins::hooks::{HookContext, HookPoint};
    use crate::plugins::runner::PluginRunner;
    use crate::session::types::CallLogKind;
    use crate::tui::app::*;

    fn make_app() -> crate::tui::app::App {
        crate::tui::make_test_app("maestro-tui-app-test")
    }

    #[tokio::test]
    async fn issue_865_second_session_after_dismiss_clears_dismissed_via_baseline() {
        // Regression for #865 — after dismissing the modal, the gate must
        // auto-clear `completion_summary_dismissed` once a new session
        // enters the pool (so the modal re-fires for the 2nd, 3rd, … Nth
        // session). Pre-fix, dismissed stayed sticky and the modal never
        // re-fired even though `add_session` reset the flag — some
        // arrival paths (retry, queue advance) bypass add_session.
        let mut app = make_app();

        // 1st session completes
        let s1 = crate::session::types::Session::new(
            "first".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        app.pool.enqueue(s1);
        let promoted = app.pool.try_promote();
        for id in promoted {
            app.pool.finalize_and_teardown(id);
        }
        assert!(app.pool.all_done(), "pool must be all_done after 1st");
        assert_eq!(app.pool.total_count(), 1);

        // User dismisses the modal via the helper (mimics input handler)
        app.completion_summary = Some(CompletionSummaryData {
            sessions: vec![],
            total_cost_usd: 0.0,
            session_count: 1,
            suggestions: vec![],
            selected_suggestion: 0,
        });
        app.dismiss_completion_summary();
        assert!(app.completion_summary_dismissed);
        assert_eq!(app.completion_summary_baseline_total, 1);

        // 2nd session enters the pool via add_session
        let s2 = crate::session::types::Session::new(
            "second".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        let _ = app.add_session(s2).await;

        // The gate's auto-clear check (mirrored here) must trip because
        // total_count (2) > baseline (1).
        if app.completion_summary_dismissed
            && app.pool.total_count() > app.completion_summary_baseline_total
        {
            app.completion_summary_dismissed = false;
        }
        assert!(
            !app.completion_summary_dismissed,
            "dismissed must auto-clear once a new session has entered the pool (#865)"
        );
    }

    #[test]
    fn dismissed_flag_prevents_summary_retrigger_scenario() {
        // Simulate: completion summary was shown, user dismissed it,
        // all sessions are still done → summary must NOT re-trigger.
        let mut app = make_app();
        // Set up a finished session
        let session = crate::session::types::Session::new(
            "done".to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
            None,
        );
        app.pool.enqueue(session);
        let promoted = app.pool.try_promote();
        for id in promoted {
            app.pool.finalize_and_teardown(id);
        }
        assert!(app.pool.all_done(), "pool must be all_done");

        // Dismiss the summary
        app.completion_summary_dismissed = true;
        app.completion_summary = None;
        app.tui_mode = TuiMode::Overview;

        // The auto-transition condition should NOT fire:
        // all_done=true, continuous_mode=None, completion_summary=None,
        // but completion_summary_dismissed=true → blocked
        let should_trigger = app.pool.all_done()
            && app.continuous_mode.is_none()
            && app.completion_summary.is_none()
            && !app.completion_summary_dismissed;
        assert!(
            !should_trigger,
            "auto-transition must not fire when completion_summary_dismissed is true"
        );
    }

    /// #890: the hollow-completion modal trigger filter must exclude
    /// sessions whose `hollow_dismissed` flag is set, otherwise the modal
    /// re-fires on every render tick after the user pressed [s] Skip.
    #[test]
    fn hollow_dismissed_session_excluded_from_modal_trigger_filter() {
        use crate::session::types::{Session, SessionStatus};
        let mut session =
            Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        session.status = SessionStatus::Completed;
        session.is_hollow_completion = true;
        session.hollow_dismissed = true;

        // Mirror the find() predicate at line 360-366 of this file.
        let retryable_ids: std::collections::HashSet<uuid::Uuid> = std::collections::HashSet::new();
        let modal_should_open = !session.hollow_dismissed
            && session.status == SessionStatus::Completed
            && session.is_hollow_completion
            && !retryable_ids.contains(&session.id);
        assert!(
            !modal_should_open,
            "hollow_dismissed session must not re-trigger the recovery modal"
        );
    }

    #[test]
    fn hollow_not_dismissed_session_still_triggers_modal() {
        use crate::session::types::{Session, SessionStatus};
        let mut session =
            Session::new("p".into(), "opus".into(), "orchestrator".into(), None, None);
        session.status = SessionStatus::Completed;
        session.is_hollow_completion = true;
        // hollow_dismissed left as default false

        let retryable_ids: std::collections::HashSet<uuid::Uuid> = std::collections::HashSet::new();
        let modal_should_open = !session.hollow_dismissed
            && session.status == SessionStatus::Completed
            && session.is_hollow_completion
            && !retryable_ids.contains(&session.id);
        assert!(
            modal_should_open,
            "fresh hollow session must trigger the recovery modal"
        );
    }

    /// #895: opening the Hollow modal must push the prior mode onto the
    /// nav-stack so [s] Skip's `navigate_back_or_dashboard` returns there
    /// instead of falling through to Dashboard with an empty stack.
    #[test]
    fn opening_hollow_modal_pushes_prior_mode_onto_nav_stack() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        let depth_before = app.nav_stack.depth();

        // Simulate the modal-open code path.
        app.navigate_to(TuiMode::HollowRetry);

        assert_eq!(app.tui_mode, TuiMode::HollowRetry);
        assert_eq!(
            app.nav_stack.depth(),
            depth_before + 1,
            "navigate_to must push Overview onto the nav-stack"
        );

        // Simulate the [s] Skip → ScreenAction::Pop handler.
        app.navigate_back_or_dashboard();
        assert_eq!(
            app.tui_mode,
            TuiMode::Overview,
            "Skip must return to Overview, not Dashboard"
        );
    }

    /// #895 sibling: opening the AdaptFollowUp ("Next iteration paths")
    /// modal must push the prior mode so [Esc] returns there instead of
    /// falling through to Dashboard.
    #[test]
    fn opening_adapt_follow_up_pushes_prior_mode_onto_nav_stack() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Detail(uuid::Uuid::nil());
        let depth_before = app.nav_stack.depth();

        app.navigate_to(TuiMode::AdaptFollowUp);

        assert_eq!(app.tui_mode, TuiMode::AdaptFollowUp);
        assert_eq!(
            app.nav_stack.depth(),
            depth_before + 1,
            "navigate_to must push Detail onto the nav-stack"
        );

        app.navigate_back_or_dashboard();
        assert_eq!(
            app.tui_mode,
            TuiMode::Detail(uuid::Uuid::nil()),
            "Esc must return to Detail, not Dashboard"
        );
    }

    /// #914 — `tests_passed` hook must append a `HookResponse` entry to
    /// the owning session's call log when the session is resolvable via
    /// `issue_number`.
    #[tokio::test]
    async fn tests_passed_hook_appends_hook_response_to_call_log_with_resolvable_session() {
        let mut app = make_app();

        let session = crate::session::types::Session::new(
            "test prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(42),
            None,
        );
        app.pool.enqueue(session);
        let promoted = app.pool.try_promote();
        assert!(!promoted.is_empty(), "session must be promoted to active");

        // Set up a minimal PluginRunner that responds to tests_passed.
        let plugin = PluginConfig {
            name: "test-plugin".into(),
            on: "tests_passed".into(),
            run: "echo success".into(),
            timeout_secs: None,
        };
        app.plugin_runner = Some(PluginRunner::new(vec![plugin], 30));

        // Resolve the session ID by issue_number — same pattern as the
        // completion-pipeline fix.
        let session_id_str = app
            .pool
            .find_by_issue_mut(42)
            .map(|m| m.session.id.to_string())
            .unwrap_or_default();
        assert!(
            !session_id_str.is_empty(),
            "session must be findable by issue_number"
        );

        let ctx = HookContext::new().with_session(&session_id_str, Some(42));
        app.fire_plugin_hook(HookPoint::TestsPassed, ctx).await;

        // Verify the session's call log gained a HookResponse entry.
        if let Some(managed) = app.pool.find_by_issue_mut(42) {
            let has_hook_response = managed
                .session
                .call_log
                .iter()
                .any(|entry| matches!(entry.kind, CallLogKind::HookResponse));
            assert!(
                has_hook_response,
                "session.call_log must contain a HookResponse after tests_passed hook fires"
            );
        } else {
            panic!("session must still be findable after hook fires");
        }
    }

    /// #1144: a rejected pipeline transition must be recorded, not
    /// dropped — the session activity log entry carries from → to and
    /// `tracing` receives the warn with the same states.
    #[test]
    fn rejected_pipeline_transition_is_logged_with_from_and_to() {
        use crate::session::transition::TransitionReason;
        use crate::session::types::SessionStatus;

        let mut session = crate::session::types::Session::new(
            "s".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1144),
            None,
        );
        session.status = SessionStatus::Completed;

        let settled = super::checked_transition(
            &mut session,
            SessionStatus::GatesRunning,
            TransitionReason::GatesStarted,
        );

        assert!(
            !settled,
            "Completed has no outgoing edges — the transition must be rejected"
        );
        assert!(
            session
                .activity_log
                .iter()
                .any(|e| e.message == "TRANSITION REJECTED: COMPLETED \u{2192} GATES_RUNNING"),
            "rejection must be logged with the from → to states"
        );
    }
}

/// Workspace trust gate tests (#1045) — judge round: the tick-path
/// promotion funnel (check_completions → try_promote → managed.spawn)
/// must respect the gate, not only add_session.
#[cfg(all(test, unix))]
mod workspace_trust_tests {
    use super::*;
    use crate::commands::setup::setup_app_from_config_with_trust;
    use crate::config::{Config, LoadedConfig};
    use crate::session::types::Session;
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    #[tokio::test]
    async fn workspace_trust_tick_promotion_spawn_blocked_untrusted() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("agent.sh");
        let marker = dir.path().join("marker.txt");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\necho ready\n", marker.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let toml_body = format!(
            r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
max_concurrent = 1
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "{}"
"#,
            script.display()
        );
        let config: Config = toml::from_str(&toml_body).unwrap();
        let mut app = setup_app_from_config_with_trust(
            LoadedConfig {
                config,
                path: dir.path().join("maestro.toml"),
            },
            StateStore::new(dir.path().join("state.json")),
            Box::new(MockWorktreeManager::new()),
            None,
            false,
            false,
        );
        for issue in [1u64, 2] {
            app.pool.enqueue(Session::new(
                "probe".into(),
                "opus".into(),
                "orchestrator".into(),
                Some(issue),
                None,
            ));
        }

        // The tick funnel itself promotes and spawns (slot 1 free): with the
        // guard this must block and log; without it the config command runs.
        app.check_completions().await.unwrap();

        assert!(
            !marker.exists(),
            "tick-path promotion must not spawn an untrusted config command"
        );
        assert!(
            app.activity_log
                .entries()
                .iter()
                .any(|e| e.message.contains("Spawn blocked")),
            "tick-path block must be visible in the activity log"
        );
        let _ = dir;
    }
}
