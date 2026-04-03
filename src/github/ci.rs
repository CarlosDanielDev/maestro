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

/// Action to take after evaluating CI failure for auto-fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiPollAction {
    /// CI still running or fix session in progress — keep waiting.
    Wait,
    /// Spawn a fix session with this failure log.
    SpawnFix { log: String },
    /// Retries exhausted or auto-fix disabled — give up.
    Abandon,
}

/// Tracks a PR awaiting CI completion.
#[derive(Debug, Clone)]
pub struct PendingPrCheck {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub created_at: std::time::Instant,
    pub check_count: u32,
    /// Number of CI fix attempts already made for this PR.
    pub fix_attempt: u32,
    /// If true, a fix session is running — skip re-processing failures.
    pub awaiting_fix_ci: bool,
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
    #[allow(dead_code)]
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
        parse_ci_json(&json_str)
    }

    /// Fetch the failed CI run log for a PR branch.
    /// Returns a truncated log string (max ~4000 chars to fit in a prompt).
    pub fn fetch_failure_log(&self, pr_number: u64, branch: &str) -> Result<String> {
        let output = std::process::Command::new("gh")
            .args([
                "run",
                "list",
                "--branch",
                branch,
                "--status",
                "failure",
                "--limit",
                "1",
                "--json",
                "databaseId",
            ])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "gh run list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let runs: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
        let run_id = runs
            .first()
            .and_then(|r| r.get("databaseId"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("No failed run found for PR #{}", pr_number))?;

        let log_output = std::process::Command::new("gh")
            .args(["run", "view", &run_id.to_string(), "--log-failed"])
            .output()?;

        if !log_output.status.success() {
            anyhow::bail!(
                "gh run view --log-failed failed: {}",
                String::from_utf8_lossy(&log_output.stderr).trim()
            );
        }

        let full_log = String::from_utf8_lossy(&log_output.stdout).to_string();
        Ok(truncate_log(&full_log, 4000))
    }
}

impl Default for CiChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse CI status JSON from `gh pr view` output.
pub(crate) fn parse_ci_json(json: &str) -> Result<CiStatus> {
    let pr_status: PrStatusJson = serde_json::from_str(json)?;

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

/// Truncate a log to the last `max_chars` characters, keeping complete lines.
pub(crate) fn truncate_log(log: &str, max_chars: usize) -> String {
    if log.len() <= max_chars {
        return log.to_string();
    }
    let raw_start = log.len() - max_chars;
    // Walk forward to find a valid UTF-8 char boundary
    let start = (raw_start..log.len())
        .find(|&i| log.is_char_boundary(i))
        .unwrap_or(log.len());
    // Find the next newline after the cut point to keep lines intact
    match log[start..].find('\n') {
        Some(pos) => format!("...(truncated)\n{}", &log[start + pos + 1..]),
        None => format!("...(truncated)\n{}", &log[start..]),
    }
}

/// A deferred CI fix request, collected during polling and processed after the loop.
pub struct CiFixRequest {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub attempt: u32,
    pub failure_log: String,
}

/// Decide what action to take when CI fails for a pending PR check.
pub fn decide_ci_action(check: &PendingPrCheck, max_retries: u32, error_log: &str) -> CiPollAction {
    if check.awaiting_fix_ci {
        return CiPollAction::Wait;
    }
    if check.fix_attempt >= max_retries {
        return CiPollAction::Abandon;
    }
    CiPollAction::SpawnFix {
        log: error_log.to_string(),
    }
}

/// Build a prompt for a CI fix session.
pub(crate) fn build_ci_fix_prompt(
    pr_number: u64,
    issue_number: u64,
    branch: &str,
    attempt: u32,
    failure_log: &str,
) -> String {
    format!(
        "Fix the CI failure for PR #{pr_number} (issue #{issue_number}).\n\n\
         This is auto-fix attempt {attempt}.\n\n\
         CI FAILURE LOG:\n```\n{failure_log}\n```\n\n\
         IMPORTANT: You are running in unattended mode. \
         Do NOT use AskUserQuestion. \
         Read the failing code, fix the issue, then commit and push to the branch '{branch}'. \
         Run the failing command locally first to reproduce, then fix and verify. \
         Keep the fix minimal — do NOT refactor unrelated code. Only fix the CI failure.",
    )
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
            fix_attempt: 0,
            awaiting_fix_ci: false,
        };
        assert_eq!(check.pr_number, 42);
        assert_eq!(check.issue_number, 10);
        assert_eq!(check.branch, "maestro/issue-10");
        assert_eq!(check.check_count, 0);
        assert_eq!(check.fix_attempt, 0);
        assert!(!check.awaiting_fix_ci);
    }

    #[test]
    fn ci_poll_action_wait_equals_wait() {
        assert_eq!(CiPollAction::Wait, CiPollAction::Wait);
    }

    #[test]
    fn ci_poll_action_abandon_ne_wait() {
        assert_ne!(CiPollAction::Abandon, CiPollAction::Wait);
    }

    #[test]
    fn truncate_log_returns_full_string_when_under_limit() {
        let log = "error: type mismatch in assignment".to_string();
        let result = truncate_log(&log, 4096);
        assert_eq!(result, log);
    }

    #[test]
    fn truncate_log_clips_long_input() {
        let log = "x".repeat(10_000);
        let result = truncate_log(&log, 2048);
        // Result should be roughly max_chars + prefix length
        assert!(result.len() <= 2048 + 20);
    }

    #[test]
    fn truncate_log_preserves_line_boundaries() {
        let log = "line1\nline2\nline3\nline4\n".repeat(500);
        let result = truncate_log(&log, 100);
        // Should start with truncation marker
        assert!(result.starts_with("...(truncated)\n"));
    }

    #[test]
    fn truncate_log_respects_utf8_char_boundaries() {
        // 'é' is 2 bytes; a 2000-char string is 4000 bytes
        let log = "é".repeat(2000);
        let result = truncate_log(&log, 2048);
        // Must be valid UTF-8 (would panic otherwise)
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn parse_ci_json_all_success_returns_passed() {
        let json = r#"{
            "statusCheckRollup": [
                {"name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
                {"name":"test","status":"COMPLETED","conclusion":"success"}
            ],
            "mergeStateStatus": "CLEAN"
        }"#;
        assert_eq!(parse_ci_json(json).unwrap(), CiStatus::Passed);
    }

    #[test]
    fn parse_ci_json_empty_rollup_returns_none_configured() {
        let json = r#"{"statusCheckRollup":[],"mergeStateStatus":"CLEAN"}"#;
        assert_eq!(parse_ci_json(json).unwrap(), CiStatus::NoneConfigured);
    }

    #[test]
    fn parse_ci_json_failure_conclusion_returns_failed_with_summary() {
        let json = r#"{
            "statusCheckRollup": [
                {"name":"build","status":"COMPLETED","conclusion":"FAILURE"},
                {"name":"lint","status":"COMPLETED","conclusion":"TIMED_OUT"}
            ],
            "mergeStateStatus": "BLOCKED"
        }"#;
        let status = parse_ci_json(json).unwrap();
        match status {
            CiStatus::Failed { summary } => {
                assert!(summary.contains("build"));
                assert!(summary.contains("lint"));
            }
            other => panic!("Expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn parse_ci_json_in_progress_returns_pending() {
        let json = r#"{
            "statusCheckRollup": [
                {"name":"build","status":"IN_PROGRESS","conclusion":""}
            ],
            "mergeStateStatus": "BLOCKED"
        }"#;
        assert_eq!(parse_ci_json(json).unwrap(), CiStatus::Pending);
    }

    #[test]
    fn parse_ci_json_mixed_success_and_pending_returns_pending() {
        let json = r#"{
            "statusCheckRollup": [
                {"name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
                {"name":"deploy","status":"QUEUED","conclusion":""}
            ],
            "mergeStateStatus": "BLOCKED"
        }"#;
        assert_eq!(parse_ci_json(json).unwrap(), CiStatus::Pending);
    }

    #[test]
    fn decide_ci_action_spawn_fix_when_under_limit() {
        let check = PendingPrCheck {
            pr_number: 42,
            issue_number: 7,
            branch: "b".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 1,
            awaiting_fix_ci: false,
        };
        let action = decide_ci_action(&check, 3, "CI failed: missing semicolon");
        assert!(matches!(action, CiPollAction::SpawnFix { .. }));
    }

    #[test]
    fn decide_ci_action_abandon_when_at_limit() {
        let check = PendingPrCheck {
            pr_number: 42,
            issue_number: 7,
            branch: "b".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 3,
            awaiting_fix_ci: false,
        };
        let action = decide_ci_action(&check, 3, "CI failed");
        assert_eq!(action, CiPollAction::Abandon);
    }

    #[test]
    fn decide_ci_action_wait_when_awaiting_fix_ci() {
        let check = PendingPrCheck {
            pr_number: 1,
            issue_number: 1,
            branch: "b".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 0,
            awaiting_fix_ci: true,
        };
        assert_eq!(decide_ci_action(&check, 3, ""), CiPollAction::Wait);
    }

    #[test]
    fn decide_ci_action_abandon_when_max_retries_is_zero() {
        let check = PendingPrCheck {
            pr_number: 1,
            issue_number: 1,
            branch: "b".into(),
            created_at: std::time::Instant::now(),
            check_count: 0,
            fix_attempt: 0,
            awaiting_fix_ci: false,
        };
        assert_eq!(decide_ci_action(&check, 0, "error"), CiPollAction::Abandon);
    }

    #[test]
    fn build_ci_fix_prompt_contains_pr_number_and_error_log() {
        let prompt = build_ci_fix_prompt(42, 7, "feat/fix", 1, "error[E0308]: mismatched types");
        assert!(prompt.contains("PR #42"));
        assert!(prompt.contains("mismatched types"));
        assert!(prompt.contains("attempt 1"));
    }

    #[test]
    fn build_ci_fix_prompt_includes_fix_scope_guard() {
        let prompt = build_ci_fix_prompt(1, 1, "branch", 1, "test failed");
        let lower = prompt.to_lowercase();
        assert!(
            lower.contains("do not") || lower.contains("only fix"),
            "Expected scope-limiting instruction in prompt, got: {}",
            prompt
        );
    }
}
