use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Trait for git operations, enabling mock injection in tests.
pub trait GitOps: Send + Sync {
    /// Stage all changes, commit, and push to remote.
    fn commit_and_push(
        &self,
        worktree_path: &Path,
        branch: &str,
        message: &str,
    ) -> Result<()>;
}

/// Production implementation using git CLI commands.
pub struct CliGitOps;

impl GitOps for CliGitOps {
    fn commit_and_push(
        &self,
        worktree_path: &Path,
        branch: &str,
        message: &str,
    ) -> Result<()> {
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
}

#[cfg(test)]
pub struct MockGitOps {
    pub should_fail: bool,
}

#[cfg(test)]
impl MockGitOps {
    pub fn new() -> Self {
        Self { should_fail: false }
    }
}

#[cfg(test)]
impl GitOps for MockGitOps {
    fn commit_and_push(
        &self,
        _worktree_path: &Path,
        _branch: &str,
        _message: &str,
    ) -> Result<()> {
        if self.should_fail {
            anyhow::bail!("mock: git operations failed");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_git_ops_succeeds_by_default() {
        let ops = MockGitOps::new();
        assert!(ops
            .commit_and_push(Path::new("/tmp"), "main", "test")
            .is_ok());
    }

    #[test]
    fn mock_git_ops_fails_when_configured() {
        let ops = MockGitOps { should_fail: true };
        assert!(ops
            .commit_and_push(Path::new("/tmp"), "main", "test")
            .is_err());
    }
}
