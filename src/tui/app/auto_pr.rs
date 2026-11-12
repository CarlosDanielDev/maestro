//! Auto-PR pipeline (#514). Owns the canonical PR URL helper and the
//! App-state snapshot (`build_pr_phase_input`) that feeds
//! `auto_pr_phase::pr_phase` — the network + git half of the flow,
//! split out in #1153 so the completion job can run it off the render
//! loop. Production runs the phase on `completion_job.rs`; every App
//! mutation lives in `auto_pr_apply::apply_pr_outcome`.

use super::App;
use super::auto_pr_phase::PrPhaseInput;
use std::path::PathBuf;

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

impl App {
    /// Snapshot everything `pr_phase` reads from App state: config
    /// flags, repo identity, auth flag and the issue cache. Shared by
    /// the inline orchestration and the #1153 completion job dispatch —
    /// the job calls this before spawning, so the phase itself touches
    /// no App state.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_pr_phase_input(
        &self,
        issue_number: u64,
        issue_numbers: Vec<u64>,
        is_unified: bool,
        success: bool,
        cost_usd: f64,
        worktree_branch: Option<String>,
        worktree_path: Option<PathBuf>,
    ) -> PrPhaseInput {
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
        let cached_primary = self.state.issue_cache.get(&issue_number).cloned();
        let cached_unified = issue_numbers
            .iter()
            .filter_map(|n| self.state.issue_cache.get(n).cloned())
            .collect();
        PrPhaseInput {
            issue_number,
            issue_numbers,
            is_unified,
            success,
            cost_usd,
            branch: worktree_branch,
            worktree_path,
            gh_auth_ok: self.gh_auth_ok,
            auto_pr,
            base_branch,
            repo_slug,
            cached_primary,
            cached_unified,
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
