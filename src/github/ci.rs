use anyhow::Result;
use serde::Deserialize;

/// CI check status for a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiStatus {
    /// Checks are still running.
    Pending,
    /// All checks passed.
    Passed,
    /// One or more checks failed.
    Failed { summary: String },
    /// No CI checks configured on this repo/branch.
    NoneConfigured,
}

/// Tracks a PR awaiting CI completion.
#[derive(Debug, Clone)]
pub struct PendingPrCheck {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub created_at: std::time::Instant,
    pub check_count: u32,
}

/// Checks CI status for pull requests via `gh` CLI.
pub struct CiChecker;

#[derive(Deserialize)]
struct PrStatusJson {
    #[serde(default)]
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Vec<CheckRun>,
    #[serde(default)]
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: String,
}

#[derive(Deserialize)]
struct CheckRun {
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
}

impl CiChecker {
    pub fn new() -> Self {
        Self
    }

    /// Check the CI status for a given PR number.
    pub fn check_pr_status(&self, pr_number: u64) -> Result<CiStatus> {
        let num_str = pr_number.to_string();
        let output = std::process::Command::new("gh")
            .args([
                "pr",
                "view",
                &num_str,
                "--json",
                "statusCheckRollup,mergeStateStatus",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gh pr view failed: {}", stderr.trim());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let pr_status: PrStatusJson = serde_json::from_str(&json_str)?;

        if pr_status.status_check_rollup.is_empty() {
            return Ok(CiStatus::NoneConfigured);
        }

        let mut pending = false;
        let mut failures = Vec::new();

        for check in &pr_status.status_check_rollup {
            match check.conclusion.as_str() {
                "SUCCESS" | "NEUTRAL" | "SKIPPED" | "success" | "neutral" | "skipped" => {}
                "" if check.status != "COMPLETED" && check.status != "completed" => {
                    pending = true;
                }
                conclusion => {
                    failures.push(format!("{}: {}", check.name, conclusion));
                }
            }
        }

        if !failures.is_empty() {
            Ok(CiStatus::Failed {
                summary: failures.join("; "),
            })
        } else if pending {
            Ok(CiStatus::Pending)
        } else {
            Ok(CiStatus::Passed)
        }
    }
}

impl Default for CiChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_status_eq() {
        assert_eq!(CiStatus::Pending, CiStatus::Pending);
        assert_eq!(CiStatus::Passed, CiStatus::Passed);
        assert_eq!(CiStatus::NoneConfigured, CiStatus::NoneConfigured);
        assert_eq!(
            CiStatus::Failed {
                summary: "test".into()
            },
            CiStatus::Failed {
                summary: "test".into()
            }
        );
        assert_ne!(CiStatus::Pending, CiStatus::Passed);
    }

    #[test]
    fn pending_pr_check_stores_fields() {
        let check = PendingPrCheck {
            pr_number: 42,
            issue_number: 10,
            branch: "maestro/issue-10".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
        };
        assert_eq!(check.pr_number, 42);
        assert_eq!(check.issue_number, 10);
        assert_eq!(check.branch, "maestro/issue-10");
        assert_eq!(check.check_count, 0);
    }
}
