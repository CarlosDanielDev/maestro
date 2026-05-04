use anyhow::{Context, Result};
use std::{future::Future, time::Duration};

pub use super::super::redaction::redact_secrets;

/// Check if a stderr string indicates a GitHub CLI authentication failure.
pub fn is_auth_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("not logged in")
        || lower.contains("authentication required")
        || lower.contains("http 401")
        || lower.contains("auth login")
        || lower.contains("try authenticating")
        || lower.contains("authentication token")
        || lower.contains("could not authenticate")
}

/// Sentinel prefix used to tag gh auth errors in anyhow messages.
pub(super) const GH_AUTH_ERROR_SENTINEL: &str = "[gh-auth-error]";
const GH_RATE_LIMIT_MAX_ATTEMPTS: u32 = 3;

pub(super) fn is_rate_limit_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("http 429")
        || lower.contains("rate limit exceeded")
        || lower.contains("secondary rate limit")
        || lower.contains("too many requests")
}

pub(super) fn rate_limit_delay_for_attempt(attempt: u32) -> Duration {
    Duration::from_millis(250u64.saturating_mul(2u64.saturating_pow(attempt)))
}

pub(super) async fn with_rate_limit_retries<F, Fut>(
    mut operation: F,
    sleep_between_attempts: bool,
) -> Result<String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Err(err)
                if attempt + 1 < GH_RATE_LIMIT_MAX_ATTEMPTS
                    && is_rate_limit_error(&err.to_string()) =>
            {
                if sleep_between_attempts {
                    tokio::time::sleep(rate_limit_delay_for_attempt(attempt)).await;
                }
                attempt += 1;
            }
            result => return result,
        }
    }
}

pub(crate) fn normalize_paginated_json_arrays(json: &str) -> Result<String> {
    let stream = serde_json::Deserializer::from_str(json).into_iter::<serde_json::Value>();
    let mut items = Vec::new();
    for value in stream {
        match value.context("Failed to parse paginated GitHub JSON")? {
            serde_json::Value::Array(page) => items.extend(page),
            other => anyhow::bail!("Expected paginated GitHub response array, got {other:?}"),
        }
    }
    serde_json::to_string(&items).context("Failed to normalize paginated GitHub JSON")
}

/// True when stderr matches `gh issue edit --remove-label`'s
/// "label missing" shape, keyed on the label literal so unrelated
/// `not found` errors (issue/repo/branch) don't trigger. Both
/// "not on repo" and "not on issue" share this stderr (gh v2.x);
/// both are no-ops for remove.
pub(super) fn is_label_not_found_error(stderr: &str, label: &str) -> bool {
    let needle = format!("'{}' not found", label);
    stderr.contains(&needle)
}

/// Extract the PR number from `gh pr create` stdout.
///
/// `gh pr create` does not accept `--json`; it prints the new PR's URL
/// (e.g. `https://github.com/owner/repo/pull/123`) on stdout, possibly
/// preceded by progress lines. We grab the last `/pull/<digits>` token.
pub(crate) fn parse_pr_number_from_create_output(stdout: &str) -> Result<u64> {
    let after_pull = stdout
        .lines()
        .filter_map(|line| line.trim().rsplit_once("/pull/").map(|(_, rest)| rest))
        .next_back()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "gh pr create did not return a /pull/ URL. stdout was: {:?}",
                stdout
            )
        })?;
    let digits: String = after_pull
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits
        .parse::<u64>()
        .with_context(|| format!("Could not parse PR number from `{}`", after_pull))
}

/// Check if an anyhow error is a gh CLI auth error (by sentinel prefix).
pub fn is_gh_auth_error(err: &anyhow::Error) -> bool {
    err.to_string().contains(GH_AUTH_ERROR_SENTINEL)
}
