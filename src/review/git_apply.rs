//! Production `GitChangeApplier` (#327).
//!
//! Applies a `Concern.suggested_diff` as a real git commit. Implements
//! the security mitigations the security review flagged (#327 §1):
//! diff size cap, path allow-list, `git apply --check` dry-run, post-apply
//! HEAD re-verification.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::review::apply::{AppliedChange, ChangeApplier, ChangeApplyError};
use crate::review::types::Concern;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Maximum allowed `suggested_diff` size in bytes. 256 KiB is generous
/// for real review patches and rules out runaway payloads.
pub const MAX_DIFF_BYTES: usize = 256 * 1024;

/// Path prefixes the applier refuses to touch even if a reviewer suggests
/// a diff against them. Order matters only for readability.
const FORBIDDEN_PATH_PREFIXES: &[&str] = &[
    ".git/",
    ".github/workflows/",
    ".github/actions/",
    "scripts/check-file-size.sh",
    "scripts/allowlist-large-files.txt",
    "Cargo.lock",
];

pub struct GitChangeApplier {
    repo_root: PathBuf,
}

impl GitChangeApplier {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    fn head_sha(&self) -> Result<String, ChangeApplyError> {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| ChangeApplyError::GitCommandFailed {
                step: "rev-parse",
                stderr: e.to_string(),
            })?;
        if !out.status.success() {
            return Err(ChangeApplyError::GitCommandFailed {
                step: "rev-parse",
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn write_patch_to_tempfile(diff: &str) -> Result<PathBuf, ChangeApplyError> {
        let tmp = std::env::temp_dir().join(format!(
            "maestro-review-patch-{}.diff",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&tmp, diff).map_err(|e| ChangeApplyError::GitCommandFailed {
            step: "write-tempfile",
            stderr: e.to_string(),
        })?;
        Ok(tmp)
    }

    fn run_git(
        &self,
        step: &'static str,
        args: &[&str],
    ) -> Result<std::process::Output, ChangeApplyError> {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| ChangeApplyError::GitCommandFailed {
                step,
                stderr: e.to_string(),
            })?;
        Ok(out)
    }
}

impl ChangeApplier for GitChangeApplier {
    fn apply(&self, concern: &Concern) -> Result<AppliedChange, ChangeApplyError> {
        let diff = concern
            .suggested_diff
            .as_ref()
            .ok_or(ChangeApplyError::NothingToApply)?;

        validate_diff(diff)?;

        let head_before = self.head_sha()?;
        let patch_path = Self::write_patch_to_tempfile(diff)?;
        let patch_path_str = patch_path.to_string_lossy();

        // Dry-run: refuse to touch the index if the patch wouldn't apply.
        let check = self.run_git("apply --check", &["apply", "--check", &patch_path_str])?;
        if !check.status.success() {
            let _ = std::fs::remove_file(&patch_path);
            return Err(ChangeApplyError::PatchFailed(
                String::from_utf8_lossy(&check.stderr).into_owned(),
            ));
        }

        let apply = self.run_git("apply", &["apply", "--index", &patch_path_str])?;
        let _ = std::fs::remove_file(&patch_path);
        if !apply.status.success() {
            return Err(ChangeApplyError::PatchFailed(
                String::from_utf8_lossy(&apply.stderr).into_owned(),
            ));
        }

        let commit_msg = format!("fix(review): address concern {}", concern.id);
        let commit = self.run_git("commit", &["commit", "-m", &commit_msg])?;
        if !commit.status.success() {
            return Err(ChangeApplyError::GitCommandFailed {
                step: "commit",
                stderr: String::from_utf8_lossy(&commit.stderr).into_owned(),
            });
        }

        let head_after = self.head_sha()?;
        if head_after == head_before {
            // Could happen if the diff was a no-op; surface as PatchFailed
            // so the caller knows nothing committed.
            return Err(ChangeApplyError::PatchFailed(
                "commit succeeded but HEAD did not advance — empty change".into(),
            ));
        }

        Ok(AppliedChange {
            concern_id: concern.id,
            commit_sha: head_after,
        })
    }
}

/// Validate the suggested diff against the security policy:
/// - Within `MAX_DIFF_BYTES`
/// - No forbidden path prefixes touched
/// - No path-traversal segments (`..`, absolute paths)
fn validate_diff(diff: &str) -> Result<(), ChangeApplyError> {
    if diff.len() > MAX_DIFF_BYTES {
        return Err(ChangeApplyError::PatchFailed(format!(
            "suggested_diff exceeds {MAX_DIFF_BYTES} bytes"
        )));
    }
    for line in diff.lines() {
        if let Some(stripped) = line.strip_prefix("+++ b/") {
            check_path_safe(stripped)?;
        } else if let Some(stripped) = line.strip_prefix("--- a/") {
            check_path_safe(stripped)?;
        }
    }
    Ok(())
}

fn check_path_safe(path: &str) -> Result<(), ChangeApplyError> {
    if path.is_empty() {
        return Ok(());
    }
    if Path::new(path).is_absolute() {
        return Err(ChangeApplyError::PatchFailed(format!(
            "diff references absolute path: {path}"
        )));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(ChangeApplyError::PatchFailed(format!(
            "diff contains path-traversal segment: {path}"
        )));
    }
    for forbidden in FORBIDDEN_PATH_PREFIXES {
        if path.starts_with(forbidden) {
            return Err(ChangeApplyError::PatchFailed(format!(
                "diff targets forbidden path: {path} (matches '{forbidden}')"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_diff_accepts_normal_patch() {
        let diff = "--- a/src/foo.rs\n+++ b/src/foo.rs\n@@ -1 +1 @@\n-old\n+new\n";
        assert!(validate_diff(diff).is_ok());
    }

    #[test]
    fn validate_diff_rejects_oversized_payload() {
        let diff = "x".repeat(MAX_DIFF_BYTES + 1);
        assert!(matches!(
            validate_diff(&diff),
            Err(ChangeApplyError::PatchFailed(_))
        ));
    }

    #[test]
    fn validate_diff_rejects_workflows_target() {
        let diff =
            "--- a/.github/workflows/ci.yml\n+++ b/.github/workflows/ci.yml\n@@ -1 +1 @@\n-x\n+y\n";
        match validate_diff(diff) {
            Err(ChangeApplyError::PatchFailed(msg)) => assert!(msg.contains("workflows")),
            other => panic!("expected forbidden-path error, got {other:?}"),
        }
    }

    #[test]
    fn validate_diff_rejects_dot_dot_path_traversal() {
        let diff = "+++ b/../etc/passwd\n";
        match validate_diff(diff) {
            Err(ChangeApplyError::PatchFailed(msg)) => assert!(msg.contains("traversal")),
            other => panic!("expected traversal error, got {other:?}"),
        }
    }

    #[test]
    fn validate_diff_rejects_absolute_path() {
        let diff = "+++ b//etc/hosts\n";
        match validate_diff(diff) {
            Err(ChangeApplyError::PatchFailed(msg)) => assert!(msg.contains("absolute")),
            other => panic!("expected absolute-path error, got {other:?}"),
        }
    }

    #[test]
    fn validate_diff_rejects_dot_git_target() {
        let diff = "--- a/.git/HEAD\n+++ b/.git/HEAD\n@@ -1 +1 @@\n-x\n+y\n";
        match validate_diff(diff) {
            Err(ChangeApplyError::PatchFailed(msg)) => assert!(msg.contains(".git/")),
            other => panic!("expected forbidden .git error, got {other:?}"),
        }
    }

    #[test]
    fn validate_diff_rejects_cargo_lock() {
        let diff = "+++ b/Cargo.lock\n";
        assert!(validate_diff(diff).is_err());
    }

    #[test]
    fn validate_diff_accepts_empty_path_lines() {
        // Edge case: a diff fragment without `--- a/` / `+++ b/` headers
        // is accepted at this layer; `git apply --check` will reject it
        // for being invalid.
        let diff = "@@ -1 +1 @@\n-old\n+new\n";
        assert!(validate_diff(diff).is_ok());
    }
}
