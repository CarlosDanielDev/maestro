use anyhow::Result;
use std::process::Command;

use crate::util::validate_branch_name;

/// Configuration for the review pipeline.
#[derive(Debug, Clone)]
pub struct ReviewConfig {
    pub enabled: bool,
    pub command: String,
}

/// Dispatches review commands after PR creation.
pub struct ReviewDispatcher {
    config: ReviewConfig,
}

impl ReviewDispatcher {
    pub fn new(config: ReviewConfig) -> Self {
        Self { config }
    }

    /// Check if review dispatch is enabled.
    #[allow(dead_code)] // Reason: review dispatch toggle — to be used in PR creation flow
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Dispatch a review command for a given PR.
    /// Template variables: {pr_number}, {branch}
    pub fn dispatch(&self, pr_number: u64, branch: &str) -> Result<ReviewResult> {
        if !self.config.enabled {
            return Ok(ReviewResult {
                success: true,
                output: "Review disabled".into(),
                reviewer_name: None,
            });
        }

        Self::run_review_command(&self.config.command, pr_number, branch, None)
    }

    /// Run a single review command with variable substitution.
    pub fn run_review_command(
        command: &str,
        pr_number: u64,
        branch: &str,
        reviewer_name: Option<&str>,
    ) -> Result<ReviewResult> {
        validate_branch_name(branch)?;

        let command = command
            .replace("{pr_number}", &pr_number.to_string())
            .replace("{branch}", branch);

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            anyhow::bail!("Empty review command");
        }

        let output = Command::new(parts[0]).args(&parts[1..]).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ReviewResult {
            success: output.status.success(),
            output: if stdout.is_empty() { stderr } else { stdout },
            reviewer_name: reviewer_name.map(|s| s.to_string()),
        })
    }

    /// Post review results as a PR comment using gh CLI.
    pub fn post_comment(pr_number: u64, body: &str) -> Result<()> {
        let pr_str = pr_number.to_string();
        let output = Command::new("gh")
            .args(["pr", "comment", &pr_str, "--body", body])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gh pr comment failed: {}", stderr);
        }

        Ok(())
    }
}

/// Result of a review command execution.
#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub success: bool,
    pub output: String,
    pub reviewer_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_disabled_returns_success() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: false,
            command: String::new(),
        });
        let result = dispatcher.dispatch(1, "main").unwrap();
        assert!(result.success);
        assert_eq!(result.output, "Review disabled");
    }

    #[test]
    fn is_enabled_reflects_config() {
        let enabled = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "echo review".into(),
        });
        assert!(enabled.is_enabled());

        let disabled = ReviewDispatcher::new(ReviewConfig {
            enabled: false,
            command: String::new(),
        });
        assert!(!disabled.is_enabled());
    }

    #[test]
    fn dispatch_with_true_command_succeeds() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "true".into(),
        });
        let result = dispatcher.dispatch(42, "maestro/issue-42").unwrap();
        assert!(result.success);
    }

    #[test]
    fn dispatch_with_false_command_fails() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "false".into(),
        });
        let result = dispatcher.dispatch(42, "maestro/issue-42").unwrap();
        assert!(!result.success);
    }

    #[test]
    fn dispatch_substitutes_template_variables() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "echo {pr_number} {branch}".into(),
        });
        let result = dispatcher.dispatch(99, "feat/test").unwrap();
        assert!(result.success);
        assert!(result.output.contains("99"));
        assert!(result.output.contains("feat/test"));
    }

    #[test]
    fn dispatch_empty_command_returns_error() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: String::new(),
        });
        assert!(dispatcher.dispatch(1, "main").is_err());
    }

    #[test]
    fn run_review_command_with_name() {
        let result =
            ReviewDispatcher::run_review_command("true", 1, "main", Some("claude")).unwrap();
        assert!(result.success);
        assert_eq!(result.reviewer_name, Some("claude".into()));
    }

    #[test]
    fn dispatch_rejects_branch_with_spaces() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "echo {branch}".into(),
        });
        assert!(dispatcher.dispatch(1, "feat branch").is_err());
    }

    #[test]
    fn dispatch_rejects_branch_with_semicolons() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "echo {branch}".into(),
        });
        assert!(dispatcher.dispatch(1, "main;rm -rf /").is_err());
    }

    #[test]
    fn dispatch_rejects_branch_with_double_dots() {
        let dispatcher = ReviewDispatcher::new(ReviewConfig {
            enabled: true,
            command: "echo {branch}".into(),
        });
        assert!(dispatcher.dispatch(1, "../../etc").is_err());
    }
}
