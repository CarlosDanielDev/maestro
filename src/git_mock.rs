//! Hand-rolled mock for `GitOps` (test-only). Lives in a sibling file
//! so `src/git.rs` can stay under the 400-LOC cap. Re-exported as
//! `crate::git::MockGitOps` via `pub use mock::MockGitOps;` in
//! `src/git.rs`.
//!
//! Capture vectors use `Arc<Mutex<...>>` so tests can clone the handle
//! before passing the mock into `App::with_git_ops` (which takes
//! ownership) and still inspect call records afterwards.

use anyhow::Result;
use std::path::Path;

use super::GitOps;

pub struct MockGitOps {
    pub should_fail: bool,
    /// Canned unified diff returned by `diff_against_merge_base` (#918).
    pub diff_text: String,
    /// Canned repo-relative file list returned by
    /// `changed_files_against_merge_base` (#1142).
    pub changed_files: Vec<String>,
    /// Recorded `(worktree_path, base_branch)` args from
    /// `diff_against_merge_base` calls (#918).
    pub diff_calls: std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, String)>>>,
    pub remote_branches: Vec<String>,
    pub commits_ahead: bool,
    /// Pre-canned answer from `head_is_wip_backup`. Tests that exercise
    /// the resume path set this to `true` before injecting the mock.
    pub head_is_wip: bool,
    /// When `true`, `backup_wip` reports `Ok(false)` (clean tree, no
    /// commit created) instead of the dirty-tree default (#1143).
    pub backup_skips: bool,
    /// Simulated network latency for push steps (#1153 tests). The
    /// mock sleeps this long in `commit_and_push` /
    /// `amend_clean_and_push` before recording the call.
    pub push_delay: Option<std::time::Duration>,
    pub backup_wip_calls: std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, u64)>>>,
    pub amend_calls: std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, String, String)>>>,
    pub commit_and_push_calls:
        std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, String, String)>>>,
}

impl MockGitOps {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            diff_text: String::new(),
            changed_files: Vec::new(),
            diff_calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            remote_branches: Vec::new(),
            commits_ahead: false,
            head_is_wip: false,
            backup_skips: false,
            push_delay: None,
            backup_wip_calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            amend_calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            commit_and_push_calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn with_commits_ahead(mut self, value: bool) -> Self {
        self.commits_ahead = value;
        self
    }

    pub fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }

    /// Configure `head_is_wip_backup` to return `value`.
    pub fn with_head_wip(mut self, value: bool) -> Self {
        self.head_is_wip = value;
        self
    }

    /// Configure `changed_files_against_merge_base` to return `files`.
    pub fn with_changed_files(mut self, files: Vec<String>) -> Self {
        self.changed_files = files;
        self
    }

    /// Configure `backup_wip` to report the #1143 clean-tree skip
    /// (`Ok(false)`, no commit created).
    pub fn with_backup_skips(mut self) -> Self {
        self.backup_skips = true;
        self
    }

    /// Simulate a slow push (#1153): both push paths sleep `secs`
    /// before succeeding, so tests can prove the caller is not blocked.
    pub fn with_slow_push(mut self, secs: u64) -> Self {
        self.push_delay = Some(std::time::Duration::from_secs(secs));
        self
    }

    fn slow_push_pause(&self) {
        if let Some(delay) = self.push_delay {
            std::thread::sleep(delay);
        }
    }
}

impl GitOps for MockGitOps {
    fn commit_and_push(&self, worktree_path: &Path, branch: &str, message: &str) -> Result<()> {
        if self.should_fail {
            anyhow::bail!("mock: git operations failed");
        }
        self.slow_push_pause();
        self.commit_and_push_calls.lock().unwrap().push((
            worktree_path.to_path_buf(),
            branch.to_string(),
            message.to_string(),
        ));
        Ok(())
    }

    fn list_remote_branches(&self, prefix: &str) -> Result<Vec<String>> {
        if self.should_fail {
            anyhow::bail!("mock: git operations failed");
        }
        Ok(self
            .remote_branches
            .iter()
            .filter(|b| b.contains(prefix))
            .cloned()
            .collect())
    }

    fn has_commits_ahead(&self, _worktree_path: &Path, _branch: &str, _base: &str) -> Result<bool> {
        if self.should_fail {
            anyhow::bail!("mock: git operations failed");
        }
        Ok(self.commits_ahead)
    }

    fn backup_wip(&self, worktree_path: &Path, issue_number: u64) -> Result<bool> {
        if self.should_fail {
            anyhow::bail!("mock: backup_wip failed");
        }
        self.backup_wip_calls
            .lock()
            .unwrap()
            .push((worktree_path.to_path_buf(), issue_number));
        Ok(!self.backup_skips)
    }

    fn amend_clean_and_push(
        &self,
        worktree_path: &Path,
        branch: &str,
        message: &str,
    ) -> Result<()> {
        if self.should_fail {
            anyhow::bail!("mock: amend_clean_and_push failed");
        }
        self.slow_push_pause();
        self.amend_calls.lock().unwrap().push((
            worktree_path.to_path_buf(),
            branch.to_string(),
            message.to_string(),
        ));
        Ok(())
    }

    fn head_is_wip_backup(&self, _worktree_path: &Path) -> Result<bool> {
        if self.should_fail {
            anyhow::bail!("mock: head_is_wip_backup failed");
        }
        Ok(self.head_is_wip)
    }

    fn diff_against_merge_base(&self, worktree_path: &Path, base_branch: &str) -> Result<String> {
        if self.should_fail {
            anyhow::bail!("mock: diff_against_merge_base failed");
        }
        self.diff_calls
            .lock()
            .unwrap()
            .push((worktree_path.to_path_buf(), base_branch.to_string()));
        Ok(self.diff_text.clone())
    }

    fn changed_files_against_merge_base(
        &self,
        _worktree_path: &Path,
        _base_branch: &str,
    ) -> Result<Vec<String>> {
        if self.should_fail {
            anyhow::bail!("mock: changed_files_against_merge_base failed");
        }
        Ok(self.changed_files.clone())
    }
}
