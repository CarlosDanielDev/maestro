use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Trait for git operations, enabling mock injection in tests.
pub trait GitOps: Send + Sync {
    /// Stage all changes, commit, and push to remote.
    fn commit_and_push(&self, worktree_path: &Path, branch: &str, message: &str) -> Result<()>;
    /// List remote branches matching a prefix.
    #[allow(dead_code)] // Reason: orphan branch cleanup feature
    fn list_remote_branches(&self, prefix: &str) -> Result<Vec<String>>;
    /// Whether `branch` has any commits beyond `base_branch`. Returns
    /// `false` when the branch tip equals base, which is the zero-commit
    /// session case for #514.
    #[allow(dead_code)] // Reason: zero-commit detection wired in #520; awaiting consumer
    fn has_commits_ahead(&self, worktree_path: &Path, branch: &str, base: &str) -> Result<bool>;
}

/// Production implementation using git CLI commands.
pub struct CliGitOps;

impl GitOps for CliGitOps {
    fn commit_and_push(&self, worktree_path: &Path, branch: &str, message: &str) -> Result<()> {
        // git add -A
        let output = Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_path)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Check if there's anything to commit
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(worktree_path)
            .output()?;
        let status_out = String::from_utf8_lossy(&status.stdout);
        if status_out.trim().is_empty() {
            // Nothing to commit — still push in case of unpushed commits
            let output = Command::new("git")
                .args(["push", "-u", "origin", branch])
                .current_dir(worktree_path)
                .output()?;
            if !output.status.success() {
                anyhow::bail!(
                    "git push failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(());
        }

        // git commit
        let output = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(worktree_path)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git commit failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // git push
        let output = Command::new("git")
            .args(["push", "-u", "origin", branch])
            .current_dir(worktree_path)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git push failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    fn list_remote_branches(&self, prefix: &str) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["branch", "-r", "--list", &format!("origin/{prefix}*")])
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git branch -r failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.contains("->"))
            .collect())
    }

    fn has_commits_ahead(&self, worktree_path: &Path, branch: &str, base: &str) -> Result<bool> {
        // Defense in depth (#514 security review INFO-2): refuse refs that
        // start with `-` so a regressed upstream branch validator can't
        // turn the rev-list range into a flag-injection vector.
        if branch.starts_with('-') || base.starts_with('-') {
            anyhow::bail!("invalid ref: branch and base must not start with `-`");
        }
        let range = format!("{}..{}", base, branch);
        let output = Command::new("git")
            .args(["rev-list", "--count", &range])
            .current_dir(worktree_path)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git rev-list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let count: u64 = stdout
            .trim()
            .parse()
            .map_err(|e| anyhow::anyhow!("parsing rev-list count `{}`: {}", stdout.trim(), e))?;
        Ok(count > 0)
    }
}

#[cfg(test)]
pub struct MockGitOps {
    pub should_fail: bool,
    pub remote_branches: Vec<String>,
    pub commits_ahead: bool,
}

#[cfg(test)]
impl MockGitOps {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            remote_branches: Vec::new(),
            commits_ahead: false,
        }
    }

    pub fn with_commits_ahead(mut self, value: bool) -> Self {
        self.commits_ahead = value;
        self
    }
}

#[cfg(test)]
impl GitOps for MockGitOps {
    fn commit_and_push(&self, _worktree_path: &Path, _branch: &str, _message: &str) -> Result<()> {
        if self.should_fail {
            anyhow::bail!("mock: git operations failed");
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_git_ops_succeeds_by_default() {
        let ops = MockGitOps::new();
        assert!(
            ops.commit_and_push(Path::new("/tmp"), "main", "test")
                .is_ok()
        );
    }

    #[test]
    fn mock_git_ops_fails_when_configured() {
        let ops = MockGitOps {
            should_fail: true,
            remote_branches: Vec::new(),
            commits_ahead: false,
        };
        assert!(
            ops.commit_and_push(Path::new("/tmp"), "main", "test")
                .is_err()
        );
    }

    // --- Issue #159: list_remote_branches tests ---

    #[test]
    fn mock_git_ops_list_remote_branches_filters_by_prefix() {
        let ops = MockGitOps {
            should_fail: false,
            remote_branches: vec![
                "origin/maestro/issue-42".to_string(),
                "origin/maestro/issue-99".to_string(),
                "origin/feat/something".to_string(),
            ],
            commits_ahead: false,
        };
        let branches = ops.list_remote_branches("maestro/issue-").unwrap();
        assert_eq!(branches.len(), 2);
        assert!(branches.contains(&"origin/maestro/issue-42".to_string()));
        assert!(branches.contains(&"origin/maestro/issue-99".to_string()));
    }

    #[test]
    fn mock_git_ops_list_remote_branches_returns_empty_when_no_match() {
        let ops = MockGitOps {
            should_fail: false,
            remote_branches: vec!["origin/feat/something".to_string()],
            commits_ahead: false,
        };
        let branches = ops.list_remote_branches("maestro/issue-").unwrap();
        assert!(branches.is_empty());
    }

    // --- Issue #514: has_commits_ahead detection ---

    #[test]
    fn mock_git_ops_has_commits_ahead_returns_false_by_default() {
        let ops = MockGitOps::new();
        assert!(
            !ops.has_commits_ahead(Path::new("/tmp"), "branch", "main")
                .unwrap()
        );
    }

    #[test]
    fn mock_git_ops_has_commits_ahead_returns_configured_value() {
        let ops = MockGitOps::new().with_commits_ahead(true);
        assert!(
            ops.has_commits_ahead(Path::new("/tmp"), "branch", "main")
                .unwrap()
        );
    }

    #[test]
    fn mock_git_ops_has_commits_ahead_propagates_should_fail() {
        let ops = MockGitOps {
            should_fail: true,
            remote_branches: Vec::new(),
            commits_ahead: false,
        };
        assert!(
            ops.has_commits_ahead(Path::new("/tmp"), "branch", "main")
                .is_err()
        );
    }
}
