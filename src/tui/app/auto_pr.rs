//! Auto-PR pipeline (#514). Owns the post-session-end flow that decides
//! whether to create a GitHub PR, looks up an existing one for the
//! branch, prints the URL on success, and surfaces every gate-skip path
//! with an explicit activity-log entry instead of failing silently.
//!
//! Lives in its own module so `issue_completion.rs` can remain a thin
//! orchestration entry point and stay under the 400-LOC budget.

use super::App;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::provider::github::ci::PendingPrCheck;
use crate::provider::github::pr::{PrCreator, PrRetryPolicy};
use crate::provider::github::types::{PendingPr, PendingPrStatus};
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;
use std::path::PathBuf;
use std::time::Instant;

/// Build a canonical GitHub PR URL from a `owner/repo` slug and PR number.
///
/// Returns `None` when the slug is not a single `owner/repo` pair.
/// Callers that get `None` should fall back to `client.get_pr(pr_number)`
/// and read `html_url`.
pub(crate) fn pr_url(repo_slug: &str, pr_number: u64) -> Option<String> {
    let (owner, repo) = crate::provider::github::types::parse_owner_repo(repo_slug).ok()?;
    Some(format!(
        "https://github.com/{}/{}/pull/{}",
        owner, repo, pr_number
    ))
}

/// Strip ASCII control characters (other than space) from external error
/// strings before they hit the activity log. Defeats terminal escape /
/// label-spoofing that could ride on `gh` or `git` stderr (LOW-1, #514
/// security review).
fn sanitize_log(s: &str) -> String {
    s.chars()
        .map(|c| if c == ' ' || !c.is_control() { c } else { ' ' })
        .collect()
}

impl App {
    /// Execute the auto-PR pipeline for a successful session-end event.
    /// Surfaces every gate-skip path with an explicit activity-log entry —
    /// no silent failures. Idempotent within the process via
    /// `attempted_pr_issue_numbers`; cross-restart idempotency relies on
    /// the AC4 PR-already-exists preflight (#514).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_auto_pr(
        &mut self,
        issue_number: u64,
        issue_numbers: Vec<u64>,
        cost_usd: f64,
        files_touched: Vec<String>,
        worktree_branch: Option<String>,
        worktree_path: Option<PathBuf>,
        is_unified: bool,
    ) {
        let auto_pr = self
            .config
            .as_ref()
            .map(|c| c.github.auto_pr)
            .unwrap_or(false);
        let base_branch = self
            .config
            .as_ref()
            .map(|c| c.project.base_branch.clone())
            .unwrap_or_else(|| "main".to_string());
        let repo_slug = self
            .config
            .as_ref()
            .map(|c| c.project.repo.clone())
            .unwrap_or_default();

        if !auto_pr {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                format!(
                    "Auto-PR disabled in config (github.auto_pr = false). Branch: {}",
                    worktree_branch.as_deref().unwrap_or("(none)")
                ),
                LogLevel::Info,
            );
            return;
        }

        let Some(branch) = worktree_branch.as_deref() else {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                format!(
                    "PR creation skipped — session #{} has no worktree branch.",
                    issue_number
                ),
                LogLevel::Error,
            );
            return;
        };

        if self.github_client.is_none() {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "PR creation skipped — GitHub client not initialized.".to_string(),
                LogLevel::Error,
            );
            return;
        }

        if !self.attempted_pr_issue_numbers.insert(issue_number) {
            return;
        }

        let client_ref = self.github_client.as_ref().unwrap();
        match client_ref.list_prs_for_branch(branch).await {
            Ok(numbers) if !numbers.is_empty() => {
                let existing = numbers[0];
                let url = match client_ref.get_pr(existing).await {
                    Ok(pr) => pr.html_url,
                    Err(_) => pr_url(&repo_slug, existing)
                        .unwrap_or_else(|| format!("(PR #{} — html_url lookup failed)", existing)),
                };
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    format!("PR already exists for branch {} — see {}", branch, url),
                    LogLevel::Info,
                );
                return;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "list_prs_for_branch failed; proceeding with create_pr");
            }
        }

        // AC3 of #514, wired in #520: short-circuit when the worktree
        // branch has no commits beyond base — defends against silently
        // pushing an empty branch and the noisy `gh pr create` 422.
        if let Some(wt_path) = worktree_path.as_ref() {
            match self
                .git_ops
                .has_commits_ahead(wt_path, branch, &base_branch)
            {
                Ok(false) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!(
                            "No commits found — skipping PR creation. Branch: {}",
                            branch
                        ),
                        LogLevel::Warn,
                    );
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Critical,
                        &format!("#{} — PR not opened", issue_number),
                        &format!(
                            "Session ended with zero commits on branch {}; no PR was created.",
                            branch
                        ),
                    );
                    return;
                }
                Ok(true) => {}
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "has_commits_ahead failed; proceeding optimistically with create_pr",
                    );
                }
            }
        }

        let cached_issue = self.state.issue_cache.get(&issue_number).cloned();
        let primary_issue: Option<crate::provider::github::types::GhIssue> = match cached_issue {
            Some(cached) => Some(cached),
            None => match self
                .github_client
                .as_ref()
                .unwrap()
                .get_issue(issue_number)
                .await
            {
                Ok(fetched) => Some(fetched),
                Err(e) => {
                    self.check_gh_auth_error(&e);
                    let err_text = sanitize_log(&e.to_string());
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!(
                            "PR creation skipped — could not fetch issue from GitHub: {}",
                            err_text
                        ),
                        LogLevel::Error,
                    );
                    self.notifications.notify(
                        crate::notifications::types::InterruptLevel::Critical,
                        &format!("#{} — PR not opened", issue_number),
                        &format!(
                            "Maestro could not draft the PR because the issue is not in cache and the GitHub fetch failed: {}",
                            err_text
                        ),
                    );
                    None
                }
            },
        };

        let Some(issue) = primary_issue else {
            return;
        };
        let client = self.github_client.as_ref().unwrap();
        let file_refs: Vec<&str> = files_touched.iter().map(|s| s.as_str()).collect();
        let pr_creator = PrCreator::new(client.as_ref(), base_branch.clone());

        let pr_result = if is_unified {
            let unified_issues: Vec<&crate::provider::github::types::GhIssue> = issue_numbers
                .iter()
                .filter_map(|n| self.state.issue_cache.get(n))
                .collect();
            if unified_issues.is_empty() {
                pr_creator
                    .create_for_issue(&issue, branch, &file_refs, cost_usd)
                    .await
            } else {
                let body = crate::provider::github::pr::build_unified_pr_body(
                    &unified_issues,
                    &file_refs,
                    cost_usd,
                );
                let refs: Vec<String> = issue_numbers.iter().map(|n| format!("#{}", n)).collect();
                let title = format!("[Maestro] Unified: {}", refs.join(", "));
                client
                    .create_pr(issue_number, &title, &body, branch, &base_branch)
                    .await
            }
        } else {
            pr_creator
                .create_for_issue(&issue, branch, &file_refs, cost_usd)
                .await
        };

        match pr_result {
            Ok(pr_num) => {
                let url = match pr_url(&repo_slug, pr_num) {
                    Some(u) => u,
                    None => match client.get_pr(pr_num).await {
                        Ok(pr) => pr.html_url,
                        Err(_) => format!("(PR #{} — html_url lookup failed)", pr_num),
                    },
                };
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    format!("PR #{} created: {}", pr_num, url),
                    LogLevel::Info,
                );
                self.ci_poller.add_check(PendingPrCheck {
                    pr_number: pr_num,
                    issue_number,
                    branch: branch.to_string(),
                    created_at: Instant::now(),
                    check_count: 0,
                    fix_attempt: 0,
                    awaiting_fix_ci: false,
                });
                self.dispatch_review(pr_num, branch, issue_number);
                let ctx = HookContext::new()
                    .with_session("", Some(issue_number))
                    .with_pr(pr_num)
                    .with_branch(branch)
                    .with_cost(cost_usd);
                self.fire_plugin_hook(HookPoint::PrCreated, ctx).await;
            }
            Err(e) => {
                let was_auth = self.check_gh_auth_error(&e);
                let policy = PrRetryPolicy::default();
                let now = chrono::Utc::now();
                let mut last_errors = std::collections::VecDeque::new();
                last_errors.push_back(e.to_string());
                let pending = PendingPr {
                    issue_number,
                    issue_numbers: issue_numbers.clone(),
                    branch: branch.to_string(),
                    base_branch: base_branch.clone(),
                    files_touched: files_touched.clone(),
                    cost_usd,
                    attempt: 0,
                    max_attempts: policy.max_attempts,
                    last_attempt_at: now,
                    next_retry_at: policy.delay_for_attempt(0).map(|d| {
                        now + chrono::Duration::from_std(d).unwrap_or(chrono::Duration::seconds(5))
                    }),
                    status: PendingPrStatus::RetryScheduled,
                    last_errors,
                    manual_retry_count: 0,
                };
                self.pending_prs.push(pending);

                if let Some(managed) = self.pool.find_by_issue_mut(issue_number) {
                    let _ = managed
                        .session
                        .transition_to(SessionStatus::NeedsPr, TransitionReason::PrNeeded);
                }

                if !was_auth {
                    let err_text = sanitize_log(&e.to_string());
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!(
                            "PR creation failed for branch {}: {}. Retrying automatically; \
                             run `gh pr create --base {} --head {}` to create manually.",
                            branch, err_text, base_branch, branch
                        ),
                        LogLevel::Warn,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn pr_url_valid_owner_repo_slug() {
        assert_eq!(
            pr_url("CarlosDanielDev/maestro", 42).as_deref(),
            Some("https://github.com/CarlosDanielDev/maestro/pull/42")
        );
    }

    #[test]
    fn pr_url_returns_none_when_slug_lacks_slash() {
        assert!(pr_url("ownerrepo", 1).is_none());
    }

    #[test]
    fn pr_url_returns_none_when_owner_is_empty() {
        assert!(pr_url("/repo", 1).is_none());
    }

    #[test]
    fn pr_url_returns_none_when_repo_is_empty() {
        assert!(pr_url("owner/", 1).is_none());
    }

    #[test]
    fn pr_url_rejects_multi_slash_slug() {
        assert!(pr_url("a/b/c", 1).is_none());
    }
}

#[cfg(test)]
#[path = "auto_pr_tests.rs"]
mod tests;
