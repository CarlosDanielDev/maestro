use super::{GH_AUTH_ERROR_SENTINEL, is_auth_error, redact_secrets, with_rate_limit_retries};
use anyhow::{Context, Result};

use crate::util::validate_gh_arg;

/// Convert a `Vec<String>` argv into `Vec<&str>` for `run_gh`.
pub(super) fn argv_refs(argv: &[String]) -> Vec<&str> {
    argv.iter().map(String::as_str).collect()
}

/// Implementation that shells out to `gh` CLI.
pub struct GhCliClient {
    /// `owner/repo` to thread through every read-only / edit shellout
    /// (`gh pr list/view`, `gh issue view/list/edit`). Without this `gh`
    /// infers the repo from the worktree's git remote which can fail
    /// silently when the worktree is in an odd state.
    repo: Option<String>,
}

impl GhCliClient {
    pub fn new() -> Self {
        Self { repo: None }
    }

    pub fn from_config_repo(repo: Option<String>) -> Self {
        let Some(repo) = repo.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()) else {
            return Self::new();
        };

        match Self::new().with_repo(repo) {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!("Ignoring invalid configured GitHub repo: {e}");
                Self::new()
            }
        }
    }

    /// Builder: thread an explicit `owner/repo` through every read-only
    /// and label-edit shellout. PR creation deliberately ignores this —
    /// see `build_create_pr_argv`.
    ///
    /// Validates the input through `validate_gh_arg` (rejects shell
    /// metacharacters and `--`-prefixed values) and enforces the
    /// `owner/repo` shape via `parse_owner_repo`.
    ///
    pub fn with_repo(mut self, repo: String) -> Result<Self> {
        validate_gh_arg(&repo, "repo")?;
        crate::provider::github::types::parse_owner_repo(&repo)
            .map_err(|e| anyhow::anyhow!("repo {:?}: {}", repo, e))?;
        self.repo = Some(repo);
        Ok(self)
    }

    pub(super) fn repo_arg(&self) -> Option<&str> {
        self.repo.as_deref()
    }

    pub(super) async fn run_gh(&self, args: &[&str]) -> Result<String> {
        self.run_gh_with_stdin(args, None).await
    }

    pub(super) async fn run_gh_with_stdin(
        &self,
        args: &[&str],
        stdin_data: Option<&[u8]>,
    ) -> Result<String> {
        with_rate_limit_retries(|| self.run_gh_with_stdin_once(args, stdin_data), true).await
    }

    async fn run_gh_with_stdin_once(
        &self,
        args: &[&str],
        stdin_data: Option<&[u8]>,
    ) -> Result<String> {
        let stdin_cfg = if stdin_data.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        };

        let mut child = tokio::process::Command::new("gh")
            .args(args)
            .stdin(stdin_cfg)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to run `gh` CLI. Is it installed?")?;

        if let Some(data) = stdin_data
            && let Some(mut stdin) = child.stdin.take()
        {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(data).await?;
        }

        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for `gh` CLI")?;

        if !output.status.success() {
            let stderr_raw = String::from_utf8_lossy(&output.stderr);
            let stderr = redact_secrets(stderr_raw.trim());
            if is_auth_error(&stderr) {
                anyhow::bail!("{} {}", GH_AUTH_ERROR_SENTINEL, stderr);
            }
            anyhow::bail!("gh command failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
