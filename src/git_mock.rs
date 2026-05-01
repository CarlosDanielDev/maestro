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
    pub remote_branches: Vec<String>,
    pub commits_ahead: bool,
    /// Pre-canned answer from `head_is_wip_backup`. Tests that exercise
    /// the resume path set this to `true` before injecting the mock.
    pub head_is_wip: bool,
    pub backup_wip_calls: std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, u64)>>>,
    pub amend_calls: std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, String, String)>>>,
    pub commit_and_push_calls:
        std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, String, String)>>>,
}

impl MockGitOps {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            remote_branches: Vec::new(),
            commits_ahead: false,
            head_is_wip: false,
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
}

impl GitOps for MockGitOps {
    fn commit_and_push(&self, worktree_path: &Path, branch: &str, message: &str) -> Result<()> {
        if self.should_fail {
            anyhow::bail!("mock: git operations failed");
        }
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

    fn backup_wip(&self, worktree_path: &Path, issue_number: u64) -> Result<()> {
        if self.should_fail {
            anyhow::bail!("mock: backup_wip failed");
        }
        self.backup_wip_calls
            .lock()
            .unwrap()
            .push((worktree_path.to_path_buf(), issue_number));
        Ok(())
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
}
