//! Auto-review pipeline triggered when a PR is detected (#327).
//!
//! Sequence:
//!   1. Spawn `claude --print "/review <pr>"` to generate a review report
//!      (or a stub fallback if Claude is not on PATH).
//!   2. Parse the report into a structured `ReviewReport`.
//!   3. Post the rendered comment back to the PR via `gh pr comment`.
//!   4. Return the parsed report so the TUI can show concerns.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::review::dispatch::ReviewDispatcher;
use crate::review::parse::{parse_review_comment, render_review_comment};
use crate::review::types::{PrNumber, ReviewReport};
use anyhow::{Context, Result};
use tokio::process::Command;

pub async fn run_review_cycle(pr_number: u64, _owner: &str, _repo: &str) -> Result<ReviewReport> {
    let raw = run_claude_review(pr_number).await?;
    let report = parse_or_seed(&raw, pr_number);
    let body = render_review_comment(&report);
    // Post via the existing dispatcher helper; tolerate failure (the
    // review still surfaces in the TUI even if the network call drops).
    if let Err(e) = ReviewDispatcher::post_comment(pr_number, &body) {
        tracing::warn!(pr = pr_number, "post_comment failed: {e}");
    }
    Ok(report)
}

/// Invoke `claude --print "/review <pr>"`. Falls back to an empty stub if
/// the CLI isn't on PATH so the rest of the pipeline keeps working.
async fn run_claude_review(pr_number: u64) -> Result<String> {
    let output = Command::new("claude")
        .args(["--print", &format!("/review {pr_number}")])
        .output()
        .await
        .with_context(|| "spawn claude --print /review");
    match output {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => Ok(String::from_utf8_lossy(&o.stderr).into_owned()),
        Err(_) => Ok(String::new()),
    }
}

fn parse_or_seed(raw: &str, pr_number: u64) -> ReviewReport {
    parse_review_comment(raw).unwrap_or_else(|_| ReviewReport::new(PrNumber(pr_number), "claude"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::types::{Concern, ConcernId, ConcernStatus, Severity};
    use std::path::PathBuf;

    #[test]
    fn parse_or_seed_returns_empty_report_on_unparseable_input() {
        let r = parse_or_seed("garbage that is not a maestro-review fence", 42);
        assert_eq!(r.pr_number, PrNumber(42));
        assert!(r.concerns.is_empty());
        assert_eq!(r.reviewer, "claude");
    }

    #[test]
    fn parse_or_seed_round_trips_a_well_formed_report() {
        let mut original = ReviewReport::new(PrNumber(7), "claude");
        original.concerns.push(Concern {
            id: ConcernId::new(),
            severity: Severity::Critical,
            file: PathBuf::from("src/lib.rs"),
            line: Some(1),
            message: "boom".into(),
            suggested_diff: None,
            status: ConcernStatus::Pending,
        });
        let body = render_review_comment(&original);
        let parsed = parse_or_seed(&body, 7);
        assert_eq!(parsed, original);
    }
}
