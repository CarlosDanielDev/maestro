//! PR-URL capture for the review-automation pipeline (#327).
//!
//! After `gh pr create`, its stdout contains a line like:
//!
//! ```text
//! https://github.com/owner/repo/pull/42
//! ```
//!
//! The session parser scans each emitted line through `PrUrlExtractor` and
//! emits a `PrCaptureEvent` when it spots one. The dispatcher then triggers
//! `/review`. Living in a small dedicated module keeps `session/manager.rs`
//! and `session/parser.rs` from growing.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #327. The extractor is invoked from the
// session parser in Phase 2; tests exercise the regex + struct today.
#![allow(dead_code)]

use crate::review::types::PrNumber;
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrCaptureEvent {
    pub pr_number: PrNumber,
    pub owner: String,
    pub repo: String,
    pub url: String,
}

/// Trait so the capturer can be faked in unit tests of the session layer.
pub trait PrUrlExtractor: Send + Sync {
    fn extract(&self, line: &str) -> Option<PrCaptureEvent>;
}

#[derive(Default)]
pub struct GitHubPrUrlExtractor;

impl GitHubPrUrlExtractor {
    pub fn new() -> Self {
        Self
    }
}

// INVARIANT: literal regex; verified by the unit tests below. Same
// `LazyLock<Regex>` shape as `src/util/validation.rs`.
#[allow(clippy::expect_used)]
static PR_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https://github\.com/([\w.\-]+)/([\w.\-]+)/pull/(\d+)")
        .expect("infallible PR URL regex literal")
});

impl PrUrlExtractor for GitHubPrUrlExtractor {
    fn extract(&self, line: &str) -> Option<PrCaptureEvent> {
        let captures = PR_URL_RE.captures(line)?;
        let owner = captures.get(1)?.as_str().to_string();
        let repo = captures.get(2)?.as_str().to_string();
        let number: u64 = captures.get(3)?.as_str().parse().ok()?;
        let url = captures.get(0)?.as_str().to_string();
        Some(PrCaptureEvent {
            pr_number: PrNumber(number),
            owner,
            repo,
            url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extractor() -> GitHubPrUrlExtractor {
        GitHubPrUrlExtractor::new()
    }

    #[test]
    fn extract_pr_url_from_line_happy_path() {
        let line = "Created PR: https://github.com/CarlosDanielDev/maestro/pull/42";
        let event = extractor().extract(line).expect("should extract");
        assert_eq!(event.pr_number, PrNumber(42));
        assert_eq!(event.owner, "CarlosDanielDev");
        assert_eq!(event.repo, "maestro");
        assert_eq!(
            event.url,
            "https://github.com/CarlosDanielDev/maestro/pull/42"
        );
    }

    #[test]
    fn extract_pr_url_no_match_returns_none() {
        let line = "Session completed successfully.";
        assert_eq!(extractor().extract(line), None);
    }

    #[test]
    fn extract_pr_url_prefers_first_match_in_line() {
        let line = "https://github.com/a/b/pull/1 see also https://github.com/c/d/pull/2";
        let event = extractor().extract(line).expect("should extract");
        assert_eq!(event.pr_number, PrNumber(1));
        assert_eq!(event.owner, "a");
        assert_eq!(event.repo, "b");
    }

    #[test]
    fn extract_pr_url_handles_dots_and_dashes_in_repo_name() {
        let line = "https://github.com/some.org/some-repo.thing/pull/7";
        let event = extractor().extract(line).expect("should extract");
        assert_eq!(event.owner, "some.org");
        assert_eq!(event.repo, "some-repo.thing");
        assert_eq!(event.pr_number, PrNumber(7));
    }

    #[test]
    fn extract_pr_url_rejects_issues_url() {
        let line = "https://github.com/owner/repo/issues/42";
        assert_eq!(extractor().extract(line), None);
    }

    #[test]
    fn extract_pr_url_rejects_huge_pr_numbers_gracefully() {
        // Numbers > u64::MAX should not panic — `parse()` returns None and
        // the function returns None.
        let line = "https://github.com/o/r/pull/99999999999999999999999999999";
        assert_eq!(extractor().extract(line), None);
    }
}
