//! Network + git phase of the auto-PR pipeline, split out of
//! `run_auto_pr` (#1153). `pr_phase` holds every step that waits on
//! `gh` or runs a git subprocess, so it can execute on a worker task
//! instead of the render loop; `auto_pr_apply::apply_pr_outcome` holds
//! every App mutation and runs inline in the caller's context. The
//! inline orchestration in `issue_completion.rs` chains the two; the
//! #1153 completion job runs only the phase and hands the outcome back
//! for a later tick to apply.

use crate::git::GitOps;
use crate::provider::RepoProvider;
use crate::provider::github::pr::{PrCreator, build_unified_pr_body};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::Instrument;

/// Everything `pr_phase` needs, snapshotted by the caller from `App`
/// (config, issue cache) plus the plain completion payload.
pub(super) struct PrPhaseInput {
    pub issue_number: u64,
    pub issue_numbers: Vec<u64>,
    pub is_unified: bool,
    pub success: bool,
    pub cost_usd: f64,
    pub branch: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub gh_auth_ok: bool,
    pub auto_pr: bool,
    pub base_branch: String,
    pub repo_slug: String,
    /// `state.issue_cache` snapshot taken by the caller.
    pub cached_primary: Option<crate::provider::types::Issue>,
    pub cached_unified: Vec<crate::provider::types::Issue>,
}

/// What the network/git phase decided. Every variant maps to exactly
/// the log/notify/state sequence the pre-split inline code executed.
pub(super) enum PrPhaseOutcome {
    /// `github.auto_pr = false`.
    Disabled { branch: Option<String> },
    /// Completion carried no worktree branch.
    NoBranch,
    /// No GitHub client initialized.
    NoClient,
    /// Auth lost and no branch — nothing to defer; log the auth skip.
    AuthSkippedNoBranch,
    /// Auth lost — PR deferred to the Shift+P recovery queue (#521).
    AuthDeferred {
        branch: String,
        diff_files: Vec<String>,
    },
    /// No auth and the session failed — nothing to defer.
    SkippedNotSuccess,
    /// Already attempted for this issue in this process (#514).
    AlreadyAttempted,
    /// A PR already exists for the branch (#514 preflight).
    ExistingPr {
        pr_number: u64,
        url: String,
        branch: String,
    },
    /// Zero commits beyond base — refuse to open an empty PR (#520).
    NoCommits { branch: String },
    /// Issue fetch (no cache) failed.
    FetchFailed { error: anyhow::Error },
    /// PR created.
    Created {
        pr_number: u64,
        url: String,
        branch: String,
    },
    /// PR creation failed — retry machinery takes over.
    Failed {
        error: anyhow::Error,
        diff_files: Vec<String>,
    },
}

/// Run a git subprocess off the caller's worker thread. Both call sites
/// sit inside `pr_phase`, which may execute on the #1153 completion job;
/// a multi-second `git diff` must not pin a runtime worker either.
async fn git_off_loop<T>(
    git: Arc<dyn GitOps>,
    body: impl FnOnce(&dyn GitOps) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T>
where
    T: Send + 'static,
{
    let join = tokio::task::spawn_blocking(move || body(git.as_ref()));
    join.await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("git task join error: {}", e)))
}

pub(super) async fn pr_phase(
    input: PrPhaseInput,
    client: Option<Arc<dyn RepoProvider>>,
    git: Arc<dyn GitOps>,
    attempted: Arc<Mutex<HashSet<u64>>>,
) -> PrPhaseOutcome {
    let span = tracing::info_span!("completion.auto_pr", issue = input.issue_number);
    pr_phase_inner(input, client, git, attempted)
        .instrument(span)
        .await
}

async fn pr_phase_inner(
    input: PrPhaseInput,
    client: Option<Arc<dyn RepoProvider>>,
    git: Arc<dyn GitOps>,
    attempted: Arc<Mutex<HashSet<u64>>>,
) -> PrPhaseOutcome {
    // Same order as before #1153 (`on_issue_session_completed` then
    // `run_auto_pr`): auth lost first — defer to the Shift+P recovery
    // queue (#521) before the `auto_pr` / branch / client checks —
    // then the success check, then the auto-PR gates.
    if !input.gh_auth_ok {
        if !input.success {
            return PrPhaseOutcome::SkippedNotSuccess;
        }
        let Some(branch) = input.branch.clone() else {
            return PrPhaseOutcome::AuthSkippedNoBranch;
        };
        let diff_files = diff_files_for_pr(
            input.worktree_path.clone(),
            &input.base_branch,
            &git,
            input.issue_number,
        )
        .await;
        return PrPhaseOutcome::AuthDeferred { branch, diff_files };
    }
    if !input.success {
        return PrPhaseOutcome::SkippedNotSuccess;
    }
    if !input.auto_pr {
        return PrPhaseOutcome::Disabled {
            branch: input.branch,
        };
    }
    let Some(branch) = input.branch.clone() else {
        return PrPhaseOutcome::NoBranch;
    };
    let Some(client) = client else {
        return PrPhaseOutcome::NoClient;
    };

    if attempted
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(input.issue_number)
    {
        // inserted: first attempt for this issue — proceed.
    } else {
        return PrPhaseOutcome::AlreadyAttempted;
    }

    // AC4 preflight (#514): reuse an existing PR for the branch.
    let existing_url = async {
        match client.list_prs_for_branch(&branch).await {
            Ok(numbers) if !numbers.is_empty() => {
                let existing = numbers[0];
                let url = match client.get_pr(existing).await {
                    Ok(pr) => pr.html_url,
                    Err(_) => super::auto_pr::pr_url(&input.repo_slug, existing)
                        .unwrap_or_else(|| format!("(PR #{} — html_url lookup failed)", existing)),
                };
                Some((existing, url))
            }
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "list_prs_for_branch failed; proceeding with create_pr"
                );
                None
            }
        }
    }
    .instrument(tracing::info_span!(
        "completion.auto_pr.existing_lookup",
        issue = input.issue_number
    ))
    .await;
    if let Some((existing, url)) = existing_url {
        return PrPhaseOutcome::ExistingPr {
            pr_number: existing,
            url,
            branch,
        };
    }

    // AC3 (#520): refuse to open a PR for an empty branch.
    if let Some(wt_path) = input.worktree_path.clone() {
        let base = input.base_branch.clone();
        let branch_for_check = branch.clone();
        let commits_ahead = async {
            git_off_loop(git.clone(), move |g| {
                g.has_commits_ahead(&wt_path, &branch_for_check, &base)
            })
            .await
        }
        .instrument(tracing::info_span!(
            "completion.auto_pr.commits_ahead",
            issue = input.issue_number
        ))
        .await;
        match commits_ahead {
            Ok(false) => {
                return PrPhaseOutcome::NoCommits { branch };
            }
            Ok(true) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "has_commits_ahead failed; proceeding optimistically with create_pr"
                );
            }
        }
    }

    let primary_issue = match input.cached_primary.clone() {
        Some(cached) => Some(cached),
        None => {
            let fetched = client
                .get_issue(input.issue_number)
                .instrument(tracing::info_span!(
                    "completion.auto_pr.fetch_issue",
                    issue = input.issue_number
                ))
                .await;
            match fetched {
                Ok(fetched) => Some(fetched),
                Err(e) => return PrPhaseOutcome::FetchFailed { error: e },
            }
        }
    };
    let Some(issue) = primary_issue else {
        return PrPhaseOutcome::SkippedNotSuccess;
    };

    // #1142: the PR body's Files Changed list must be the real PR diff,
    // never the session's `files_touched`. On lookup failure the section
    // is omitted: publishing no list beats publishing a local layout.
    let diff_files = diff_files_for_pr(
        input.worktree_path.clone(),
        &input.base_branch,
        &git,
        input.issue_number,
    )
    .await;
    let file_refs: Vec<&str> = diff_files.iter().map(|s| s.as_str()).collect();
    let pr_creator = PrCreator::new(client.as_ref(), input.base_branch.clone());

    let created = async {
        if input.is_unified && !input.cached_unified.is_empty() {
            let unified_issues: Vec<&crate::provider::types::Issue> = input
                .issue_numbers
                .iter()
                .filter_map(|n| input.cached_unified.iter().find(|i| i.number == *n))
                .collect();
            if unified_issues.is_empty() {
                pr_creator
                    .create_for_issue(&issue, &branch, &file_refs, input.cost_usd)
                    .await
            } else {
                let body = build_unified_pr_body(&unified_issues, &file_refs, input.cost_usd);
                let refs: Vec<String> = input
                    .issue_numbers
                    .iter()
                    .map(|n| format!("#{n}"))
                    .collect();
                let title = format!("[Maestro] Unified: {}", refs.join(", "));
                client
                    .create_pr(
                        input.issue_number,
                        &title,
                        &body,
                        &branch,
                        &input.base_branch,
                    )
                    .await
            }
        } else {
            pr_creator
                .create_for_issue(&issue, &branch, &file_refs, input.cost_usd)
                .await
        }
    }
    .instrument(tracing::info_span!(
        "completion.auto_pr.create_pr",
        issue = input.issue_number
    ))
    .await;

    match created {
        Ok(pr_num) => {
            let url = match super::auto_pr::pr_url(&input.repo_slug, pr_num) {
                Some(u) => u,
                None => match client.get_pr(pr_num).await {
                    Ok(pr) => pr.html_url,
                    Err(_) => format!("(PR #{pr_num} — html_url lookup failed)"),
                },
            };
            PrPhaseOutcome::Created {
                pr_number: pr_num,
                url,
                branch,
            }
        }
        Err(e) => PrPhaseOutcome::Failed {
            error: e,
            diff_files,
        },
    }
}

/// Repo-relative diff list for the PR body (#1142), computed off loop.
async fn diff_files_for_pr(
    worktree_path: Option<PathBuf>,
    base_branch: &str,
    git: &Arc<dyn GitOps>,
    issue_number: u64,
) -> Vec<String> {
    let Some(wt) = worktree_path else {
        return Vec::new();
    };
    let base = base_branch.to_string();
    match git_off_loop(git.clone(), move |g| {
        g.changed_files_against_merge_base(&wt, &base)
    })
    .instrument(tracing::info_span!(
        "completion.auto_pr.diff_files",
        issue = issue_number
    ))
    .await
    {
        Ok(files) => files,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "changed_files lookup failed; PR body will omit Files Changed"
            );
            Vec::new()
        }
    }
}
