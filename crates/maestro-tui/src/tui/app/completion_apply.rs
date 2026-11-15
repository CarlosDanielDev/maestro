//! #1153 — apply tick for finished completion jobs. Every App mutation
//! a [`CompletionJobOutcome`] implies, replayed in the same order the
//! pre-split inline pipeline produced it: backup-phase logs, gate
//! results and transitions, push outcome, completion side effects,
//! label outcome, CI-fix shortcut, auto-PR outcome.

use super::App;
use super::auto_pr_apply::PrApplyCtx;
use super::completion_job::CompletionJobOutcome;
use super::completion_pipeline::checked_transition;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;

impl App {
    /// Apply one finished completion job's outcome on the render loop's
    /// tick. Cheap by construction: no subprocess, no network — the job
    /// already ran all of that off loop.
    pub(super) async fn apply_completion_outcome(&mut self, out: CompletionJobOutcome) {
        let completion = out.completion;
        let issue_number = completion.issue_number;
        let issue_label = format!("#{issue_number}");

        // 1. Backup-phase activity lines, in event order.
        for (label, message, level) in out.deferred_logs {
            self.activity_log.push_simple(label, message, level);
        }

        // 2. Gate results: per-gate logs, then the pass/fail branch
        // (transitions, session log, retention, hooks) exactly as the
        // inline pipeline ordered them.
        if let Some(all_required_passed) = out.all_required_passed {
            let fail_level = if all_required_passed {
                LogLevel::Warn
            } else {
                LogLevel::Error
            };
            for (result, _) in &out.gate_results {
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
                let failures: Vec<String> = out
                    .gate_results
                    .iter()
                    .filter(|(r, required)| !r.passed && *required)
                    .map(|(r, _)| r.message.clone())
                    .collect();

                let failed_gate_results: Vec<crate::session::types::GateResultEntry> = out
                    .gate_results
                    .iter()
                    .filter(|(r, _)| !r.passed)
                    .map(|(r, _)| crate::session::types::GateResultEntry {
                        gate: r.gate.clone(),
                        passed: r.passed,
                        message: r.message.clone(),
                    })
                    .collect();

                if let Some(managed) = self.pool.find_by_issue_mut(issue_number) {
                    managed.session.gate_results = failed_gate_results;
                    // Persist the retained worktree path on the session so
                    // the post-failure recovery modal (issue #560) can
                    // surface it for [s] / [g] / [r] without piping it
                    // through the activity log.
                    managed.session.worktree_path = completion.worktree_path.clone();
                    checked_transition(
                        &mut managed.session,
                        SessionStatus::FailedGates,
                        TransitionReason::GatesFailed,
                    );
                    managed
                        .session
                        .log_activity(format!("Gates failed: {}", failures.join("; ")));
                    if let Some(wt_path) = &completion.worktree_path {
                        let retain_msg =
                            format!("Worktree retained at {} for recovery", wt_path.display());
                        managed.session.log_activity(retain_msg.clone());
                        tracing::warn!(
                            issue = issue_number,
                            worktree = %wt_path.display(),
                            "retaining worktree after gate failure",
                        );
                        self.activity_log.push_simple(
                            issue_label.clone(),
                            retain_msg,
                            LogLevel::Warn,
                        );
                    }
                }

                let session_id = self
                    .pool
                    .find_by_issue_mut(issue_number)
                    .map(|m| m.session.id.to_string())
                    .unwrap_or_default();
                let ctx = HookContext::new()
                    .with_session(&session_id, Some(issue_number))
                    .with_var("MAESTRO_GATE_FAILURES", &failures.join("; "));
                self.fire_plugin_hook(HookPoint::TestsFailed, ctx).await;
            } else {
                self.activity_log.push_simple(
                    issue_label.clone(),
                    "All required gates passed".into(),
                    LogLevel::Info,
                );
                // #1144: settle COMPLETED only now — the gates ran
                // with the session parked in GATES_RUNNING, so
                // history records the documented
                // RUNNING → GATES_RUNNING → COMPLETED flow.
                if let Some(managed) = self.pool.find_by_issue_mut(issue_number)
                    && managed.session.status == SessionStatus::GatesRunning
                {
                    checked_transition(
                        &mut managed.session,
                        SessionStatus::Completed,
                        TransitionReason::GatesPassed,
                    );
                }
                let session_id = self
                    .pool
                    .find_by_issue_mut(issue_number)
                    .map(|m| m.session.id.to_string())
                    .unwrap_or_default();
                let ctx = HookContext::new().with_session(&session_id, Some(issue_number));
                self.fire_plugin_hook(HookPoint::TestsPassed, ctx).await;
            }
        }

        // 3. Push outcome — `push_outcome` is Some exactly when the
        // pre-split pipeline would have pushed (success + branch +
        // worktree).
        if let Some(push) = out.push_outcome {
            if push.committed_clean {
                if let Some(branch) = &completion.worktree_branch {
                    self.activity_log.push_simple(
                        format!("#{issue_number}"),
                        format!("Pushed to branch {branch}"),
                        LogLevel::Info,
                    );
                }
            } else if let Some(err) = push.error_message {
                self.activity_log.push_simple(
                    format!("#{issue_number}"),
                    format!("Git push failed: {err}"),
                    LogLevel::Error,
                );
            }
        }

        // 4. Work-assigner and continuous-mode bookkeeping (cheap).
        self.apply_issue_completion_side_effects(issue_number, completion.success);

        // 5. Label-phase outcome (auth-skip log + per-error auth check).
        if let Some(labels) = &out.labels {
            self.apply_labels_outcome(labels, issue_number);
        }

        // 6. CI fix sessions skip PR creation — the PR already exists.
        if completion.is_ci_fix {
            self.activity_log.push_simple(
                format!("#{issue_number}"),
                "CI fix pushed to existing PR branch".into(),
                LogLevel::Info,
            );
            return;
        }

        // 7. Auto-PR outcome.
        if let Some(pr_outcome) = out.pr {
            let ctx = PrApplyCtx {
                issue_number,
                issue_numbers: completion.issue_numbers.clone(),
                cost_usd: completion.cost_usd,
                branch: completion.worktree_branch.clone(),
                base_branch: self
                    .config
                    .as_ref()
                    .map(|c| c.project.base_branch.clone())
                    .unwrap_or_else(|| "main".to_string()),
            };
            self.apply_pr_outcome(pr_outcome, ctx).await;
        }
    }
}
