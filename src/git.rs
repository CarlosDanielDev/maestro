use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Canonical WIP backup commit subject. Producer (`backup_wip`) and
/// detector (`head_is_wip_backup`) both reference these so they can
/// never drift.
pub const WIP_SUBJECT_PREFIX: &str = "WIP: maestro session #";
pub const WIP_SUBJECT_SUFFIX: &str = " backup before gates [skip ci]";

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

    /// Stage all changes and create an `--allow-empty` WIP backup
    /// commit. Subject embeds `issue_number` via [`WIP_SUBJECT_PREFIX`]
    /// / [`WIP_SUBJECT_SUFFIX`] so [`head_is_wip_backup`] can recognize
    /// it later. Does NOT push — the WIP lives locally until the
    /// gate-pass path amends and pushes.
    fn backup_wip(&self, worktree_path: &Path, issue_number: u64) -> Result<()>;

    /// Replace the WIP commit at HEAD with a clean conventional-commit
    /// message and push `branch` to origin. Equivalent to
    /// `git add -A && git commit --amend --allow-empty -m <message>`
    /// followed by `git push -u --force-with-lease origin <branch>`.
    fn amend_clean_and_push(&self, worktree_path: &Path, branch: &str, message: &str)
    -> Result<()>;

    /// Return `true` iff HEAD's subject matches the canonical WIP
    /// backup pattern. Returns `Ok(false)` (not `Err`) when the
    /// worktree is missing or the branch has no commits — both are
    /// valid "no WIP at HEAD" answers.
    fn head_is_wip_backup(&self, worktree_path: &Path) -> Result<bool>;
}

/// Production implementation using git CLI commands.
pub struct CliGitOps;

impl GitOps for CliGitOps {
    fn commit_and_push(&self, worktree_path: &Path, branch: &str, message: &str) -> Result<()> {
        // Refuse refs starting with `-` so a regressed validator can't
        // turn the bare branch positional into a flag-injection vector.
        if branch.starts_with('-') {
            anyhow::bail!("invalid branch ref: must not start with `-`");
        }

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

        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(worktree_path)
            .output()?;
        let status_out = String::from_utf8_lossy(&status.stdout);
        if status_out.trim().is_empty() {
            // Nothing to commit — still push in case of unpushed commits.
            // `--` separates the branch positional from any flags.
            let output = Command::new("git")
                .args(["push", "-u", "origin", "--", branch])
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

        let output = Command::new("git")
            .args(["push", "-u", "origin", "--", branch])
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

    fn backup_wip(&self, worktree_path: &Path, issue_number: u64) -> Result<()> {
        if !worktree_path.exists() {
            anyhow::bail!(
                "worktree path missing for WIP backup: {}",
                worktree_path.display()
            );
        }

        let subject = format!(
            "{}{}{}",
            WIP_SUBJECT_PREFIX, issue_number, WIP_SUBJECT_SUFFIX
        );

        let add = Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_path)
            .output()
            .with_context(|| format!("running git add -A in {}", worktree_path.display()))?;
        if !add.status.success() {
            anyhow::bail!(
                "git add failed (WIP backup): {}",
                String::from_utf8_lossy(&add.stderr).trim()
            );
        }

        let commit = Command::new("git")
            .args(["commit", "--allow-empty", "-m", &subject])
            .current_dir(worktree_path)
            .output()
            .with_context(|| {
                format!(
                    "running git commit (WIP backup) in {}",
                    worktree_path.display()
                )
            })?;
        if !commit.status.success() {
            anyhow::bail!(
                "git commit (WIP backup) failed: {}",
                String::from_utf8_lossy(&commit.stderr).trim()
            );
        }
        Ok(())
    }

    fn amend_clean_and_push(
        &self,
        worktree_path: &Path,
        branch: &str,
        message: &str,
    ) -> Result<()> {
        if branch.starts_with('-') {
            anyhow::bail!("invalid branch ref: must not start with `-`");
        }
        if !worktree_path.exists() {
            anyhow::bail!(
                "worktree path missing for amend: {}",
                worktree_path.display()
            );
        }

        let add = Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_path)
            .output()
            .with_context(|| {
                format!(
                    "running git add -A (pre-amend) in {}",
                    worktree_path.display()
                )
            })?;
        if !add.status.success() {
            anyhow::bail!(
                "git add (pre-amend) failed: {}",
                String::from_utf8_lossy(&add.stderr).trim()
            );
        }

        let amend = Command::new("git")
            .args(["commit", "--amend", "--allow-empty", "-m", message])
            .current_dir(worktree_path)
            .output()
            .with_context(|| {
                format!("running git commit --amend in {}", worktree_path.display())
            })?;
        if !amend.status.success() {
            anyhow::bail!(
                "git commit --amend failed: {}",
                String::from_utf8_lossy(&amend.stderr).trim()
            );
        }

        // `--force-with-lease` is required because amend rewrites the
        // commit hash; the lease keeps a concurrent push-then-amend
        // race from clobbering an unexpected remote tip. `--` separates
        // the branch positional from flags so a regressed validator
        // can't turn it into a flag-injection vector.
        let push = Command::new("git")
            .args(["push", "-u", "--force-with-lease", "origin", "--", branch])
            .current_dir(worktree_path)
            .output()
            .with_context(|| format!("running git push (post-amend) for {}", branch))?;
        if !push.status.success() {
            anyhow::bail!(
                "git push (post-amend) failed: {}",
                String::from_utf8_lossy(&push.stderr).trim()
            );
        }
        Ok(())
    }

    fn head_is_wip_backup(&self, worktree_path: &Path) -> Result<bool> {
        if !worktree_path.exists() {
            return Ok(false);
        }
        // One subprocess. Non-zero exit means HEAD is unborn (fresh
        // branch, no commits) — a valid "no WIP at HEAD" answer.
        // Using exit-code as the signal is locale-resilient (no
        // stderr-grepping for English error strings).
        let output = Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(worktree_path)
            .output()
            .with_context(|| format!("reading git HEAD subject in {}", worktree_path.display()))?;
        if !output.status.success() {
            return Ok(false);
        }
        let subject = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(subject.starts_with(WIP_SUBJECT_PREFIX) && subject.ends_with(WIP_SUBJECT_SUFFIX))
    }
}

#[cfg(test)]
#[path = "git_mock.rs"]
pub mod mock;

#[cfg(test)]
pub use mock::MockGitOps;

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
