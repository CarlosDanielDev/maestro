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
/// - bare `Bearer <token>` values (JWT/OAuth style)
/// - GitHub token prefixes: `ghp_`, `gho_`, `ghs_`, `ghu_`, `ghr_`,
///   and `github_pat_<v2>`
/// - Anthropic/OpenAI-style keys: `sk-` followed by 20+ key characters
///   (covers `sk-ant-…`, `sk-proj-…`, legacy `sk-…`)
/// - AWS access keys: `AKIA…` / `ASIA…` + 16 uppercase alphanumerics
///
/// `KEY=value` lines whose key names look secret keep the key name and
/// lose the value. A name qualifies when one of KEY, TOKEN, SECRET,
/// PASSWORD, PASSWD, CREDENTIAL(S) or AUTH appears at an underscore
/// boundary, ends the name, or is followed only by digits
/// (case-insensitive, so `GITHUB_TOKEN=` and `db_password=` both match,
/// while `AUTHOR=`, `KEYBOARD=` and `TOKENIZER=` stay readable). The
/// value may be bare, single- or double-quoted; a quoted value masks
/// through its closing quote (escaped quotes included) or through end
/// of line when the quote never closes.
pub fn redact_secrets(s: &str) -> String {
    static NAMED_VALUE: OnceLock<regex::Regex> = OnceLock::new();
    static TOKEN: OnceLock<regex::Regex> = OnceLock::new();
    let named_value = NAMED_VALUE.get_or_init(|| {
        regex::Regex::new(
            r#"\b((?:[A-Za-z][A-Za-z0-9_]*?)?(?i:KEY|TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIALS?|AUTH)(?:_[A-Za-z0-9_]*|[0-9]+)?)=(?:"(?:[^"\\\n]|\\.)*(?:"|(?m:$))|'(?:[^'\\\n]|\\.)*(?:'|(?m:$))|\S+)"#,
        )
        .unwrap()
    });
    let token = TOKEN.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(?:authorization:\s*(?:bearer|token)\s+\S+|bearer\s+[A-Za-z0-9._~+/=-]{16,}|gh[oprsu]_[A-Za-z0-9]+|github_pat_[A-Za-z0-9_]+|\bsk-[A-Za-z0-9_-]{20,}|\b(?:AKIA|ASIA)[0-9A-Z]{16})",
        )
        .unwrap()
    });
    let named_masked = named_value.replace_all(s, "${1}=[REDACTED]");
    token.replace_all(&named_masked, "[REDACTED]").into_owned()
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

    /// Table test for the shapes gate output actually leaks: LLM provider
    /// keys, AWS access keys, bare `Bearer` tokens, and `KEY=value` lines
    /// whose key names look secret (#1445).
    #[test]
    fn redact_secrets_common_shapes() {
        let cases = [
            (
                "anthropic key",
                "key sk-ant-api03-1234abcdefABCDEF-ghijklmnopq broke",
                "sk-ant-api03-1234abcdefABCDEF-ghijklmnopq",
                "[REDACTED]",
            ),
            (
                "openai key",
                "key sk-4fGh8iJk9Lm0nOp1qRs2tUv3wXyZ4aB5cD6eF7gH8iJk9Lm0 broke",
                "sk-4fGh8iJk9Lm0nOp1qRs2tUv3wXyZ4aB5cD6eF7gH8iJk9Lm0",
                "[REDACTED]",
            ),
            (
                "openai project key",
                "key sk-proj-3xYz9AbC2dEf1GhI5jKl6mNo7pQr8sTu broke",
                "sk-proj-3xYz9AbC2dEf1GhI5jKl6mNo7pQr8sTu",
                "[REDACTED]",
            ),
            (
                "aws access key",
                "key AKIAIOSFODNN7EXAMPLE in logs",
                "AKIAIOSFODNN7EXAMPLE",
                "[REDACTED]",
            ),
            (
                "aws temp key",
                "key ASIAQRSTUVWXYZ123456 in logs",
                "ASIAQRSTUVWXYZ123456",
                "[REDACTED]",
            ),
            (
                "bearer token",
                "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVP",
                "eyJhbGciOiJIUzI1NiJ9",
                "[REDACTED]",
            ),
            (
                "env api key",
                "ANTHROPIC_API_KEY=sk-ant-api03-1234abcdefABCDEF-ghijklmnopq",
                "sk-ant-api03-1234abcdefABCDEF-ghijklmnopq",
                "ANTHROPIC_API_KEY=[REDACTED]",
            ),
            (
                "env aws secret",
                "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                "AWS_SECRET_ACCESS_KEY=[REDACTED]",
            ),
            (
                "env token",
                "GITHUB_TOKEN=ghs_somevalue123456",
                "ghs_somevalue123456",
                "GITHUB_TOKEN=[REDACTED]",
            ),
            (
                "env password",
                "DATABASE_PASSWORD=hunter2",
                "hunter2",
                "DATABASE_PASSWORD=[REDACTED]",
            ),
        ];
        for (label, input, secret, masked) in cases {
            let out = redact_secrets(input);
            assert!(!out.contains(secret), "{label}: secret survived: {out}");
            assert!(out.contains(masked), "{label}: expected {masked} in {out}");
        }
    }

    /// Cargo output full of hashes and SHAs must pass through untouched —
    /// the new shapes must not over-mask (#1445).
    #[test]
    fn redact_secrets_keeps_plain_cargo_output() {
        let cargo = "\
   Compiling maestro-core v0.1.0 (/Users/carlos/projects/maestro-1445/crates/maestro-core)
warning: unused variable: `paddle`
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.42s
rustc 1.89.0 (dead00beef 2025-06-14)
commit 5f2a9c1d3e7b8a90f4e2d6c8b1a3f5e7d9c0b2a4
short a1b2c3d, 12 of 384 checks passed
";
        assert_eq!(redact_secrets(cargo), cargo);
    }

    /// Quoted values with spaces, and lowercase `.env`-style names, must
    /// be fully masked (#1445 round 2).
    #[test]
    fn redacts_quoted_values_and_lowercase_names() {
        let cases = [
            (
                "single-quoted with spaces",
                "PASSWORD='my secret pass'",
                "my secret pass",
                "PASSWORD=[REDACTED]",
            ),
            (
                "double-quoted with spaces",
                "SECRET_KEY=\"two words here\"",
                "two words here",
                "SECRET_KEY=[REDACTED]",
            ),
            (
                "lowercase .env name",
                "db_password=hunter2",
                "hunter2",
                "db_password=[REDACTED]",
            ),
            (
                "lowercase api key",
                "api_key=abc123def",
                "abc123def",
                "api_key=[REDACTED]",
            ),
            (
                "lowercase quoted with spaces",
                "db_password='two words'",
                "two words",
                "db_password=[REDACTED]",
            ),
        ];
        for (label, input, _secret, masked) in cases {
            let out = redact_secrets(input);
            // Exact equality: a partial mask (e.g. `\S+` stopping at the
            // first space) must fail, not just a full leak.
            assert_eq!(out, masked, "{label}: expected {masked}, got {out}");
        }
    }

    /// Look-alikes of the new shapes must stay readable (#1445 round 2):
    /// short `sk-` words, bare `bearer` in prose, names that merely
    /// contain a secret keyword without an underscore boundary, and
    /// lowercase words starting with `asia`.
    #[test]
    fn redact_secrets_keeps_lookalikes() {
        let lookalikes = "\
sk-test is a local fake key
bearer authentication required
AUTHOR=carlos
CARGO_PKG_AUTHORS=Carlos Daniel <carlos@example.com>
KEYBOARD=us
TOKENIZER=qwen
asianfoodmarket street food
";
        assert_eq!(redact_secrets(lookalikes), lookalikes);
    }

    /// The bare-`Bearer` floor is 16 characters: 15 chars stay readable,
    /// 16 get masked (#1445 round 2).
    #[test]
    fn bare_bearer_floor_is_sixteen_chars() {
        let kept = redact_secrets("Bearer AbCdEf123456789");
        assert_eq!(kept, "Bearer AbCdEf123456789");
        let masked = redact_secrets("Bearer AbCdEf1234567890");
        assert!(!masked.contains("AbCdEf1234567890"));
        assert!(masked.contains("[REDACTED]"));
    }

    /// Digit suffixes after the keyword are still secret-looking
    /// (`PASSWORD1=`), and a value that opens with a quote masks through
    /// its closing quote — escaped quotes included — or through end of
    /// line when the quote never closes (#1445 round 2).
    #[test]
    fn redacts_digit_suffixes_and_open_ended_quotes() {
        let cases = [
            ("digit suffix", "PASSWORD1=hunter2", "PASSWORD1=[REDACTED]"),
            (
                "escaped quote inside value",
                "API_KEY=\"a\\\"b\" rest",
                "API_KEY=[REDACTED] rest",
            ),
            (
                "unterminated quote",
                "SECRET='two open words",
                "SECRET=[REDACTED]",
            ),
        ];
        for (label, input, masked) in cases {
            let out = redact_secrets(input);
            assert_eq!(out, masked, "{label}: expected {masked}, got {out}");
        }
    }
}
