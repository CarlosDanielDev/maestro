use super::*;

// -- is_auth_error --

#[test]
fn is_auth_error_returns_true_for_not_logged_in() {
    assert!(is_auth_error("ERROR: not logged in to any GitHub host"));
}

#[test]
fn is_auth_error_returns_true_for_authentication_required() {
    assert!(is_auth_error("gh: authentication required"));
}

#[test]
fn is_auth_error_returns_true_for_http_401() {
    assert!(is_auth_error("HTTP 401: Unauthorized"));
}

#[test]
fn is_auth_error_returns_true_for_auth_token_errors() {
    assert!(is_auth_error(
        "error refreshing authentication token: token expired"
    ));
}

#[test]
fn is_auth_error_returns_true_for_try_authenticating() {
    assert!(is_auth_error("try authenticating with: gh auth login"));
}

#[test]
fn is_auth_error_returns_false_for_network_timeout() {
    assert!(!is_auth_error("dial tcp: connection timed out"));
}

#[test]
fn is_auth_error_returns_false_for_branch_not_found() {
    assert!(!is_auth_error("ERROR: branch 'maestro/issue-99' not found"));
}

#[test]
fn is_auth_error_returns_false_for_empty_string() {
    assert!(!is_auth_error(""));
}

#[test]
fn is_auth_error_is_case_insensitive() {
    assert!(is_auth_error("NOT LOGGED IN TO ANY GITHUB HOST"));
    assert!(is_auth_error("Http 401: unauthorized"));
    assert!(is_auth_error("AUTHENTICATION REQUIRED"));
}

// -- is_gh_auth_error --

#[test]
fn is_gh_auth_error_returns_true_for_sentinel() {
    let err = anyhow::anyhow!("[gh-auth-error] not logged in");
    assert!(is_gh_auth_error(&err));
}

#[test]
fn is_gh_auth_error_returns_false_for_regular_error() {
    let err = anyhow::anyhow!("gh command failed: branch not found");
    assert!(!is_gh_auth_error(&err));
}

// -- is_label_not_found_error (#559) --

#[test]
fn is_label_not_found_error_matches_quoted_label() {
    let stderr = "failed to update https://github.com/foo/bar: 'maestro:in-progress' not found";
    assert!(is_label_not_found_error(stderr, "maestro:in-progress"));
}

#[test]
fn is_label_not_found_error_rejects_issue_not_found() {
    let stderr = "GraphQL: Could not resolve to an Issue";
    assert!(!is_label_not_found_error(stderr, "maestro:in-progress"));
}

#[test]
fn is_label_not_found_error_rejects_label_mismatch() {
    let stderr = "'maestro:done' not found";
    assert!(!is_label_not_found_error(stderr, "maestro:in-progress"));
}

#[test]
fn is_label_not_found_error_rejects_auth_shape() {
    let stderr = "[gh-auth-error] gh auth status failed";
    assert!(!is_label_not_found_error(stderr, "maestro:in-progress"));
}

#[test]
fn is_label_not_found_error_matches_issue_url_form() {
    let stderr = "failed to update https://github.com/CarlosDanielDev/maestro/issues/542: \
                  'maestro:in-progress' not found";
    assert!(is_label_not_found_error(stderr, "maestro:in-progress"));
}

#[test]
fn is_label_not_found_error_rejects_case_mismatch() {
    let stderr = "'maestro:in-progress' not found";
    assert!(!is_label_not_found_error(stderr, "Maestro:In-Progress"));
}

#[test]
fn is_label_not_found_error_rejects_empty_stderr() {
    assert!(!is_label_not_found_error("", "maestro:in-progress"));
}

// -- PR create output / retry / pagination helpers --
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn extracts_number_from_url_only() {
    let out = "https://github.com/owner/repo/pull/123\n";
    assert_eq!(parse_pr_number_from_create_output(out).unwrap(), 123);
}

#[test]
fn extracts_number_with_preceding_progress_lines() {
    let out = "Creating pull request for foo into main in owner/repo\n\
               \n\
               https://github.com/owner/repo/pull/4242\n";
    assert_eq!(parse_pr_number_from_create_output(out).unwrap(), 4242);
}

#[test]
fn extracts_number_with_trailing_whitespace() {
    let out = "  https://github.com/owner/repo/pull/9  ";
    assert_eq!(parse_pr_number_from_create_output(out).unwrap(), 9);
}

#[test]
fn ignores_query_string_and_fragment_after_number() {
    let out = "https://github.com/owner/repo/pull/77?foo=bar#anchor";
    assert_eq!(parse_pr_number_from_create_output(out).unwrap(), 77);
}

#[test]
fn errors_on_empty_stdout() {
    assert!(parse_pr_number_from_create_output("").is_err());
}

#[test]
fn errors_when_no_pull_url_present() {
    let out = "Some unrelated diagnostic text\nNo URL here\n";
    assert!(parse_pr_number_from_create_output(out).is_err());
}

#[test]
fn picks_last_pull_url_when_multiple_present() {
    let out = "previous: https://github.com/o/r/pull/1\nfinal: https://github.com/o/r/pull/2\n";
    assert_eq!(parse_pr_number_from_create_output(out).unwrap(), 2);
}

#[tokio::test]
async fn retries_once_after_rate_limit_error() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_rate_limit_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    anyhow::bail!("gh command failed: HTTP 429 too many requests");
                }
                Ok("ok".to_string())
            }
        },
        false,
    )
    .await
    .unwrap();

    assert_eq!(result, "ok");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[test]
fn pagination_normalizes_concatenated_json_arrays() {
    let normalized = normalize_paginated_json_arrays(
        r#"[{"number":1,"title":"one"}][{"number":2,"title":"two"}]"#,
    )
    .unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&normalized).unwrap();

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["number"], 1);
    assert_eq!(parsed[1]["number"], 2);
}
