#![allow(dead_code)]
use anyhow::Result;
use serde::Deserialize;

/// Merge state of a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeState {
    Clean,
    Conflicting,
    Blocked,
    Unknown,
}

/// Conflict status for a single PR.
#[derive(Debug, Clone)]
pub struct PrConflictStatus {
    pub pr_number: u64,
    pub issue_number: u64,
    pub branch: String,
    pub merge_state: MergeState,
    pub conflicting_files: Vec<String>,
}

/// Trait for checking PR merge/conflict status. Mockable for tests.
pub trait PrMergeCheck: Send {
    fn check_merge_status(&self, pr_number: u64, issue_number: u64) -> Result<PrConflictStatus>;
}

/// Real implementation using `gh pr view` and `git diff`.
pub struct PrMergeChecker;

/// JSON shape from `gh pr view --json headRefName,mergeable,mergeStateStatus`
#[derive(Deserialize)]
struct PrMergeJson {
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    mergeable: String,
    #[serde(rename = "mergeStateStatus", default)]
    merge_state_status: String,
}

impl PrMergeChecker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PrMergeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl PrMergeCheck for PrMergeChecker {
    fn check_merge_status(&self, pr_number: u64, issue_number: u64) -> Result<PrConflictStatus> {
        let num_str = pr_number.to_string();
        let output = std::process::Command::new("gh")
            .args([
                "pr",
                "view",
                &num_str,
                "--json",
                "headRefName,mergeable,mergeStateStatus",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gh pr view failed: {}", stderr.trim());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let mut status = parse_merge_json(&json_str, pr_number, issue_number)?;

        // If conflicting, get the list of conflicting files
        if status.merge_state == MergeState::Conflicting {
            let diff_output = std::process::Command::new("git")
                .args(["diff", "--name-only", &format!("main...{}", status.branch)])
                .output()?;

            if diff_output.status.success() {
                let diff_str = String::from_utf8_lossy(&diff_output.stdout);
                status.conflicting_files = parse_conflicting_files(&diff_str);
            }
        }

        Ok(status)
    }
}

/// Parse merge status JSON from `gh pr view` output.
pub(crate) fn parse_merge_json(
    json: &str,
    pr_number: u64,
    issue_number: u64,
) -> Result<PrConflictStatus> {
    let pr: PrMergeJson = serde_json::from_str(json)?;

    let merge_state = match pr.mergeable.as_str() {
        "MERGEABLE" => {
            if pr.merge_state_status == "BLOCKED" {
                MergeState::Blocked
            } else {
                MergeState::Clean
            }
        }
        "CONFLICTING" => MergeState::Conflicting,
        _ => MergeState::Unknown,
    };

    Ok(PrConflictStatus {
        pr_number,
        issue_number,
        branch: pr.head_ref_name,
        merge_state,
        conflicting_files: Vec::new(),
    })
}

/// Parse file paths from `git diff --name-only` output.
pub(crate) fn parse_conflicting_files(diff_output: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    diff_output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| seen.insert(l.to_string()))
        .map(|l| l.to_string())
        .collect()
}

/// Build a prompt for a conflict-fix session.
pub fn build_conflict_fix_prompt(
    pr_number: u64,
    issue_number: u64,
    branch: &str,
    conflicting_files: &[String],
) -> String {
    let file_list = if conflicting_files.is_empty() {
        "  (no specific files listed)".to_string()
    } else {
        conflicting_files
            .iter()
            .map(|f| format!("- {}", f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "Fix merge conflicts in PR #{pr_number} (issue #{issue_number}) against the main branch.\n\n\
         Branch: {branch}\n\
         Conflicting files:\n{file_list}\n\n\
         Steps:\n\
         1. Run `git fetch origin main` and `git merge origin/main`\n\
         2. Resolve all merge conflicts in the listed files\n\
         3. Ensure the code compiles (`cargo build`)\n\
         4. Run tests (`cargo test`)\n\
         5. Commit the merge resolution and push\n\n\
         IMPORTANT: You are running in unattended mode. \
         Do NOT use AskUserQuestion. \
         Keep the fix minimal — do NOT refactor unrelated code. Only resolve the merge conflicts.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_merge_json_mergeable_clean_returns_clean() {
        let json =
            r#"{"headRefName":"feat/auth","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN"}"#;
        let status = parse_merge_json(json, 42, 10).unwrap();
        assert_eq!(status.merge_state, MergeState::Clean);
    }

    #[test]
    fn parse_merge_json_conflicting_returns_conflicting() {
        let json =
            r#"{"headRefName":"feat/auth","mergeable":"CONFLICTING","mergeStateStatus":"DIRTY"}"#;
        let status = parse_merge_json(json, 42, 10).unwrap();
        assert_eq!(status.merge_state, MergeState::Conflicting);
    }

    #[test]
    fn parse_merge_json_mergeable_blocked_returns_blocked() {
        let json =
            r#"{"headRefName":"feat/auth","mergeable":"MERGEABLE","mergeStateStatus":"BLOCKED"}"#;
        let status = parse_merge_json(json, 42, 10).unwrap();
        assert_eq!(status.merge_state, MergeState::Blocked);
    }

    #[test]
    fn parse_merge_json_unknown_returns_unknown() {
        let json =
            r#"{"headRefName":"feat/auth","mergeable":"UNKNOWN","mergeStateStatus":"BEHIND"}"#;
        let status = parse_merge_json(json, 42, 10).unwrap();
        assert_eq!(status.merge_state, MergeState::Unknown);
    }

    #[test]
    fn parse_merge_json_populates_pr_and_issue_numbers() {
        let json =
            r#"{"headRefName":"fix/bug","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN"}"#;
        let status = parse_merge_json(json, 99, 55).unwrap();
        assert_eq!(status.pr_number, 99);
        assert_eq!(status.issue_number, 55);
    }

    #[test]
    fn parse_merge_json_populates_branch_from_head_ref() {
        let json = r#"{"headRefName":"feat/my-branch","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN"}"#;
        let status = parse_merge_json(json, 1, 1).unwrap();
        assert_eq!(status.branch, "feat/my-branch");
    }

    #[test]
    fn parse_merge_json_invalid_json_returns_error() {
        let result = parse_merge_json("{broken", 1, 1);
        assert!(result.is_err());
    }

    #[test]
    fn parse_merge_json_conflicting_files_empty_when_not_conflicting() {
        let json = r#"{"headRefName":"fix/x","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN"}"#;
        let status = parse_merge_json(json, 1, 1).unwrap();
        assert!(status.conflicting_files.is_empty());
    }

    #[test]
    fn parse_conflicting_files_extracts_paths_from_diff_output() {
        let diff = "src/config.rs\nsrc/main.rs\nCargo.toml\n";
        let files = parse_conflicting_files(diff);
        assert_eq!(files, vec!["src/config.rs", "src/main.rs", "Cargo.toml"]);
    }

    #[test]
    fn parse_conflicting_files_empty_string_returns_empty_vec() {
        let files = parse_conflicting_files("");
        assert!(files.is_empty());
    }

    #[test]
    fn parse_conflicting_files_ignores_blank_lines() {
        let diff = "src/a.rs\n\n\nsrc/b.rs\n";
        let files = parse_conflicting_files(diff);
        assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn parse_conflicting_files_deduplicates_paths() {
        let diff = "src/a.rs\nsrc/a.rs\nsrc/b.rs\n";
        let files = parse_conflicting_files(diff);
        assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn merge_state_variants_are_distinct() {
        assert_ne!(MergeState::Clean, MergeState::Conflicting);
        assert_ne!(MergeState::Clean, MergeState::Blocked);
        assert_ne!(MergeState::Clean, MergeState::Unknown);
        assert_ne!(MergeState::Conflicting, MergeState::Blocked);
    }

    #[test]
    fn pr_conflict_status_stores_all_fields() {
        let status = PrConflictStatus {
            pr_number: 42,
            issue_number: 10,
            branch: "feat/test".to_string(),
            merge_state: MergeState::Conflicting,
            conflicting_files: vec!["src/a.rs".to_string()],
        };
        assert_eq!(status.pr_number, 42);
        assert_eq!(status.issue_number, 10);
        assert_eq!(status.branch, "feat/test");
        assert_eq!(status.merge_state, MergeState::Conflicting);
        assert_eq!(status.conflicting_files.len(), 1);
    }

    // --- build_conflict_fix_prompt tests ---

    #[test]
    fn build_conflict_fix_prompt_contains_pr_number() {
        let prompt = build_conflict_fix_prompt(42, 10, "feat/fix", &["src/a.rs".to_string()]);
        assert!(prompt.contains("PR #42"));
    }

    #[test]
    fn build_conflict_fix_prompt_contains_issue_number() {
        let prompt = build_conflict_fix_prompt(42, 10, "feat/fix", &["src/a.rs".to_string()]);
        assert!(prompt.contains("issue #10"));
    }

    #[test]
    fn build_conflict_fix_prompt_contains_branch_name() {
        let prompt = build_conflict_fix_prompt(1, 1, "feat/my-branch", &[]);
        assert!(prompt.contains("feat/my-branch"));
    }

    #[test]
    fn build_conflict_fix_prompt_lists_conflicting_files() {
        let files = vec!["src/config.rs".to_string(), "src/main.rs".to_string()];
        let prompt = build_conflict_fix_prompt(1, 1, "feat/x", &files);
        assert!(prompt.contains("src/config.rs"));
        assert!(prompt.contains("src/main.rs"));
    }

    #[test]
    fn build_conflict_fix_prompt_with_empty_files_does_not_panic() {
        let prompt = build_conflict_fix_prompt(1, 1, "feat/x", &[]);
        assert!(!prompt.is_empty());
    }
}
