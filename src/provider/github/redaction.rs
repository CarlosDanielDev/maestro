//! Scrub credentials from `gh` stderr/stdout before they reach error
//! messages, activity logs, or `maestro-state.json`.
//!
//! Lives in its own module so that lower layers (`types.rs`, anything
//! that rehydrates persisted state) can call it without depending on
//! the higher-layer `client.rs`. The redaction is gh-specific so it
//! belongs under `provider/github`, not `util`.

use std::sync::OnceLock;

/// Substitute `[REDACTED]` for any of:
/// - `Authorization: Bearer <token>` / `Authorization: token <token>`
///   (gh emits these when the user enables `GH_DEBUG=api`)
/// - GitHub token prefixes: `ghp_`, `gho_`, `ghs_`, `ghu_`, `ghr_`,
///   and `github_pat_<v2>`
pub fn redact_secrets(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(?:authorization:\s*(?:bearer|token)\s+\S+|gh[oprsu]_[A-Za-z0-9]+|github_pat_[A-Za-z0-9_]+)",
        )
        .unwrap()
    });
    re.replace_all(s, "[REDACTED]").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_bearer_token() {
        let out = redact_secrets("Authorization: Bearer ghp_realtoken1234567890ABCDEFGH");
        assert!(!out.contains("ghp_realtoken1234567890ABCDEFGH"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_pat_v2() {
        let out = redact_secrets("oops github_pat_11ABCDEFG_xxxxxxxxxxx leaked");
        assert!(!out.contains("github_pat_11ABCDEFG_xxxxxxxxxxx"));
    }

    #[test]
    fn passes_through_clean_text() {
        let out = redact_secrets("plain error message with no secrets");
        assert_eq!(out, "plain error message with no secrets");
    }
}
