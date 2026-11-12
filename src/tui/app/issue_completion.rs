use super::App;
#[cfg(test)]
use super::auto_pr_apply::PrApplyCtx;
#[cfg(test)]
use super::auto_pr_phase::pr_phase;
use super::types::TuiMode;
use crate::provider::RepoProvider;
use crate::provider::github::labels::LabelManager;
use crate::tui::activity_log::LogLevel;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;
use tracing::Instrument;

/// Result of the label-update phase (`labels_phase`): everything that
/// waits on GitHub, separated from the App mutations in
/// `apply_labels_outcome` (#1153).
pub(super) struct LabelsOutcome {
    /// Client present but `gh_auth_ok` false — the caller logs the skip.
    pub auth_skipped: bool,
    pub errors: Vec<(u64, anyhow::Error)>,
}

/// Run the label lifecycle for a completed session. Awaits GitHub for
/// every issue; contains no App state, so the #1153 completion job can
/// run it on a worker task and hand the outcome to a later tick.
pub(super) async fn labels_phase(
    client: Option<Arc<dyn RepoProvider>>,
    gh_auth_ok: bool,
    issues_to_label: Vec<u64>,
    success: bool,
) -> LabelsOutcome {
    labels_phase_inner(client, gh_auth_ok, issues_to_label, success)
        .instrument(tracing::info_span!("completion.labels"))
        .await
}

async fn labels_phase_inner(
    client: Option<Arc<dyn RepoProvider>>,
    gh_auth_ok: bool,
    issues_to_label: Vec<u64>,
    success: bool,
) -> LabelsOutcome {
    let Some(client) = client else {
        return LabelsOutcome {
            auth_skipped: false,
            errors: Vec::new(),
        };
    };
    if !gh_auth_ok {
        return LabelsOutcome {
            auth_skipped: true,
            errors: Vec::new(),
        };
    }
    let label_mgr = LabelManager::new(client.as_ref());
    let mut errors = Vec::new();
    for num in issues_to_label {
        let result = if success {
            label_mgr.mark_done(num).await
        } else {
            label_mgr.mark_failed(num).await
        };
        if let Err(e) = result {
            errors.push((num, e));
        }
    }
    LabelsOutcome {
        auth_skipped: false,
        errors,
    }
}

impl App {
    /// Cheap side effects of a session completion: work-assigner
    /// dependency graph and continuous-mode bookkeeping. Split from the
    /// GitHub tail so the #1153 apply phase can run it inline while the
    /// network work happens on the completion job.
    pub(super) fn apply_issue_completion_side_effects(&mut self, issue_number: u64, success: bool) {
        // Update work assigner
        if let Some(ref mut service) = self.work_assignment_service {
            if success {
                let unblocked = service.inner_mut().mark_done(issue_number);
                if !unblocked.is_empty() {
                    let nums: Vec<String> = unblocked
                        .iter()
                        .map(|i| format!("#{}", i.number()))
                        .collect();
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Unblocked: {}", nums.join(", ")),
                        LogLevel::Info,
                    );
                }
            } else {
                let cascaded = service.inner_mut().mark_failed_cascade(issue_number);
                if !cascaded.is_empty() {
                    let nums: Vec<String> = cascaded.iter().map(|n| format!("#{n}")).collect();
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Cascade failed: {}", nums.join(", ")),
                        LogLevel::Error,
                    );
                    // Emit critical notification for cascaded failures
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Critical,
                        &format!("#{} failed", issue_number),
                        &format!(
                            "Blocked {} dependent task{}: {}",
                            cascaded.len(),
                            if cascaded.len() != 1 { "s" } else { "" },
                            nums.join(", ")
                        ),
                    );
                }
            }
        }

        // Continuous mode: track completion/failure
        if let Some(ref mut cont) = self.continuous_mode {
            if success {
                cont.on_issue_completed(issue_number);
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!(
                        "Issue #{} completed ({} done so far)",
                        issue_number, cont.completed_count
                    ),
                    LogLevel::Info,
                );
            } else {
                let title = self
                    .state
                    .issue_cache
                    .get(&issue_number)
                    .map(|i| i.title.clone())
                    .unwrap_or_else(|| format!("Issue #{}", issue_number));
                let entries = self.activity_log.entries();
                let error_summary = entries
                    .iter()
                    .rev()
                    .take(10)
                    .find(|e| e.level == LogLevel::Error)
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "Session failed".into());
                cont.on_issue_failed(issue_number, title, error_summary);
                self.tui_mode = TuiMode::ContinuousPause;
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Issue #{} failed — paused for user decision", issue_number),
                    LogLevel::Warn,
                );
            }
        }
    }

    /// Log the label-phase outcome (auth skip + per-error auth check
    /// and activity entries). Mirrors the pre-split inline sequence.
    pub(super) fn apply_labels_outcome(&mut self, outcome: &LabelsOutcome, issue_number: u64) {
        if outcome.auth_skipped {
            self.log_gh_auth_skip(issue_number, "label update");
        }
        for (num, e) in &outcome.errors {
            self.check_gh_auth_error(e);
            self.activity_log.push_simple(
                format!("#{}", num),
                format!("Label update failed: {}", e),
                LogLevel::Error,
            );
        }
    }

    /// Inline phase → apply chain. Test-only since #1153: production
    /// dispatches a `CompletionJob` and applies its outcome later.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub async fn on_issue_session_completed(
        &mut self,
        issue_number: u64,
        issue_numbers: Vec<u64>,
        success: bool,
        cost_usd: f64,
        worktree_branch: Option<String>,
        worktree_path: Option<PathBuf>,
        is_ci_fix: bool,
    ) {
        let is_unified = issue_numbers.len() >= 2;
        self.apply_issue_completion_side_effects(issue_number, success);

        // Update GitHub labels (for all issues in unified sessions)
        if self.github_client.is_some() {
            let client = self.github_client.clone();
            let issues_to_label = if is_unified {
                issue_numbers.clone()
            } else {
                vec![issue_number]
            };
            let labels = labels_phase(client, self.gh_auth_ok, issues_to_label, success).await;
            self.apply_labels_outcome(&labels, issue_number);
        }

        // CI fix sessions skip PR creation — the PR already exists
        if is_ci_fix {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "CI fix pushed to existing PR branch".into(),
                LogLevel::Info,
            );
            return;
        }

        // Auto PR pipeline. `pr_phase` holds every `gh` call and git
        // subprocess (including the auth-deferred diff lookup, #521);
        // `apply_pr_outcome` mutates App state inline (#1153).
        let input = self.build_pr_phase_input(
            issue_number,
            issue_numbers.clone(),
            is_unified,
            success,
            cost_usd,
            worktree_branch.clone(),
            worktree_path,
        );
        let outcome = pr_phase(
            input,
            self.github_client.clone(),
            self.git_ops.clone(),
            std::sync::Arc::clone(&self.attempted_pr_issue_numbers),
        )
        .await;
        let ctx = PrApplyCtx {
            issue_number,
            issue_numbers,
            cost_usd,
            branch: worktree_branch,
            base_branch: self
                .config
                .as_ref()
                .map(|c| c.project.base_branch.clone())
                .unwrap_or_else(|| "main".to_string()),
        };
        self.apply_pr_outcome(outcome, ctx).await;
    }
}

#[cfg(test)]
#[path = "issue_completion_tests.rs"]
mod tests;
