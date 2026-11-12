//! #1153 — the completion job. Runs the heavy tail of a pending issue
//! completion — WIP backup, gates, git push, GitHub labels, the auto-PR
//! phase — on a spawned worker task so the render loop keeps drawing
//! while the work runs (the reported freeze was 9+ s of gates/push/
//! GitHub timeouts holding `check_completions` between draws).
//!
//! The job owns only clones (`Arc`) and plain data snapshotted at
//! dispatch. Every App mutation is carried back in a
//! [`CompletionJobOutcome`] that `check_completions` drains on a later
//! tick via `completion_apply.rs::apply_completion_outcome`.
//!
//! Blocking git/gate work runs inside `spawn_blocking` so a slow
//! subprocess never pins a tokio worker; GitHub calls await directly in
//! the job task.

use super::auto_pr_phase::{PrPhaseInput, PrPhaseOutcome, pr_phase};
use super::completion_git::{CompletionGitOutcome, DeferredLog, run_backup_phase, run_push_phase};
use super::issue_completion::LabelsOutcome;
use super::issue_completion::labels_phase;
use crate::gates::runner::{GateCheck, GateRunner};
use crate::gates::types::CompletionGate;
use crate::gates::types::GateResult;
use crate::git::GitOps;
use crate::provider::RepoProvider;
use crate::state::types::PendingIssueCompletion;
use crate::tui::activity_log::LogLevel;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Everything the apply tick needs to replay one finished completion:
/// the (post-gate-updated) payload, per-phase results and logs, in the
/// order the pre-split inline pipeline produced them.
pub(super) struct CompletionJobOutcome {
    /// The enqueued completion with `success` updated by the gate run.
    pub completion: PendingIssueCompletion,
    /// `None` when gates did not run.
    pub all_required_passed: Option<bool>,
    /// Per-gate results with the required flag, only when gates ran.
    pub gate_results: Vec<(GateResult, bool)>,
    /// Backup-phase activity-log lines, in event order.
    pub deferred_logs: Vec<DeferredLog>,
    /// `Some` exactly when the pre-split pipeline would have pushed.
    pub push_outcome: Option<CompletionGitOutcome>,
    /// `None` when no GitHub client was configured at dispatch.
    pub labels: Option<LabelsOutcome>,
    /// `None` for CI-fix completions (no PR flow).
    pub pr: Option<PrPhaseOutcome>,
}

/// Snapshot of one pending completion plus the Arc clones it needs.
/// Built by `App::dispatch_completion_job` and handed to a tokio task.
pub(super) struct CompletionJob {
    pub completion: PendingIssueCompletion,
    pub gates: Vec<CompletionGate>,
    /// Decided once at dispatch, where the GATES_RUNNING re-arm uses it.
    pub gates_will_run: bool,
    pub git: Arc<dyn GitOps>,
    pub github: Option<Arc<dyn RepoProvider>>,
    pub gh_auth_ok: bool,
    pub pr_input: PrPhaseInput,
    pub attempted: Arc<Mutex<HashSet<u64>>>,
    pub sink: Arc<Mutex<Vec<CompletionJobOutcome>>>,
}

impl CompletionJob {
    /// Spawn the job. Requires a tokio runtime — both production call
    /// sites (the TUI event loop) and tests run inside one.
    pub(super) fn dispatch(self) {
        tracing::debug!(
            issue = self.completion.issue_number,
            "dispatching completion job off the render loop"
        );
        tokio::spawn(self.run());
    }

    async fn run(self) {
        let sink = Arc::clone(&self.sink);
        let outcome = self.execute().await;
        sink.lock().unwrap_or_else(|e| e.into_inner()).push(outcome);
    }

    async fn execute(self) -> CompletionJobOutcome {
        let issue = self.completion.issue_number;
        let mut completion = self.completion;
        let worktree = completion.worktree_path.clone();
        let branch = completion.worktree_branch.clone();
        let gates_ran = self.gates_will_run;

        // Sync phases (backup, gates, push) run on the blocking pool:
        // they shell out to `git` and gate commands via
        // `std::process::Command`.
        let git = Arc::clone(&self.git);
        let gates = self.gates;
        let mut pr_input = self.pr_input;
        let issue_for_sync = issue;
        let success_in = completion.success;
        let unified = completion.issue_numbers.len() >= 2;
        let issue_numbers = completion.issue_numbers.clone();
        let sync = tokio::task::spawn_blocking(move || {
            let _span =
                tracing::info_span!("completion.sync_phases", issue = issue_for_sync).entered();

            let (head_is_wip, deferred_logs) = match worktree.as_ref() {
                Some(wt) => {
                    let _span =
                        tracing::info_span!("completion.backup_wip", issue = issue_for_sync)
                            .entered();
                    run_backup_phase(git.as_ref(), issue_for_sync, wt)
                }
                None => (false, Vec::new()),
            };

            let gate_worktree = worktree.as_deref().filter(|_| gates_ran);
            let (gate_results, all_required_passed) = if let Some(wt) = gate_worktree {
                let _span =
                    tracing::info_span!("completion.gates", issue = issue_for_sync).entered();
                let results: Vec<(GateResult, bool)> = GateRunner
                    .run_gates(&gates, wt)
                    .into_iter()
                    .zip(gates.iter().map(|g| g.is_required()))
                    .collect();
                let all_required_passed = crate::gates::types::all_required_gates_passed(&results);
                (results, Some(all_required_passed))
            } else {
                (Vec::new(), None)
            };

            // Push only on success — mirrors the pre-split pipeline,
            // where a gate failure set `success = false` before the
            // push block.
            let success_after_gates = all_required_passed.unwrap_or(success_in);
            let push_outcome = if success_after_gates
                && let (Some(wt), Some(branch)) = (worktree.as_deref(), branch.as_deref())
            {
                let commit_msg = if unified {
                    let refs: Vec<String> = issue_numbers.iter().map(|n| format!("#{n}")).collect();
                    format!("feat: implement unified changes for {}", refs.join(", "))
                } else {
                    format!("feat: implement changes for issue #{}", issue_for_sync)
                };
                let _span =
                    tracing::info_span!("completion.push", issue = issue_for_sync).entered();
                Some(run_push_phase(
                    git.as_ref(),
                    wt,
                    branch,
                    &commit_msg,
                    head_is_wip,
                ))
            } else {
                None
            };

            (
                success_after_gates,
                deferred_logs,
                gate_results,
                all_required_passed,
                push_outcome,
            )
        });

        let (success_after_gates, deferred_logs, gate_results, all_required_passed, push_outcome) =
            match sync.await {
                Ok(parts) => parts,
                Err(e) => {
                    // Blocking phase panicked — record and fail the step
                    // rather than silently dropping the completion.
                    tracing::error!(issue = issue, error = %e, "completion job sync phases panicked");
                    panic_outcome_parts(issue, gates_ran, format!("Completion job failed: {}", e))
                }
            };

        completion.success = success_after_gates;

        // GitHub labels — run regardless of the gate outcome (a failed
        // session still gets marked failed on the issue).
        let labels = match self.github.clone() {
            Some(client) => {
                let issues_to_label = if completion.issue_numbers.len() >= 2 {
                    completion.issue_numbers.clone()
                } else {
                    vec![issue]
                };
                let outcome = labels_phase(
                    Some(client),
                    self.gh_auth_ok,
                    issues_to_label,
                    completion.success,
                )
                .await;
                // A label auth failure must suppress the PR attempt,
                // exactly as the pre-split inline sequence did
                // (labels ran before the auth check in the PR section).
                let auth_lost_by_labels = outcome
                    .errors
                    .iter()
                    .any(|(_, e)| crate::provider::github::client::is_gh_auth_error(e));
                pr_input.gh_auth_ok = self.gh_auth_ok && !auth_lost_by_labels;
                Some(outcome)
            }
            None => {
                pr_input.gh_auth_ok = self.gh_auth_ok;
                None
            }
        };

        // Auto-PR phase — CI-fix completions skip the PR flow entirely.
        let pr = if completion.is_ci_fix {
            None
        } else {
            pr_input.success = completion.success;
            Some(
                pr_phase(
                    pr_input,
                    self.github.clone(),
                    self.git.clone(),
                    self.attempted,
                )
                .await,
            )
        };

        CompletionJobOutcome {
            completion,
            all_required_passed,
            gate_results,
            deferred_logs,
            push_outcome,
            labels,
            pr,
        }
    }
}

/// Results of the blocking phases: success after gates, backup logs,
/// gate results, gate verdict, push outcome.
type SyncParts = (
    bool,
    Vec<DeferredLog>,
    Vec<(GateResult, bool)>,
    Option<bool>,
    Option<CompletionGitOutcome>,
);

/// Outcome parts for a job whose blocking phase died: fail the gate
/// step, surface the panic in the activity log.
fn panic_outcome_parts(issue_number: u64, gates_ran: bool, message: String) -> SyncParts {
    let label = format!("#{issue_number}");
    (
        false,
        vec![(label, message, LogLevel::Error)],
        Vec::new(),
        if gates_ran { Some(false) } else { None },
        None,
    )
}
